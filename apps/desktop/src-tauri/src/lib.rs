use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::prelude::*;
use chrono::{SecondsFormat, Utc};
use regex::Regex;
use reqwest::blocking::Client;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};
use thiserror::Error;
use zip::ZipArchive;

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
    sidecar: Arc<Mutex<SidecarManager>>,
}

impl AppState {
    fn new(
        db_path: PathBuf,
        seed: &str,
        sidecar_command: String,
        sidecar_cwd: PathBuf,
        preferred_sidecar_port: u16,
    ) -> Self {
        Self {
            db_path: Arc::new(db_path),
            cipher: Arc::new(FieldCipher::from_seed(seed)),
            sidecar: Arc::new(Mutex::new(SidecarManager {
                command: sidecar_command,
                cwd: sidecar_cwd,
                preferred_port: preferred_sidecar_port,
                active_port: preferred_sidecar_port,
                child: None,
                last_error: None,
                restart_count: 0,
            })),
        }
    }
}

#[derive(Debug)]
struct SidecarManager {
    command: String,
    cwd: PathBuf,
    preferred_port: u16,
    active_port: u16,
    child: Option<Child>,
    last_error: Option<String>,
    restart_count: u32,
}

#[derive(Debug, Clone, Serialize)]
struct SidecarRuntime {
    ok: bool,
    port: u16,
    base_url: String,
    source: String,
    message: Option<String>,
    restart_count: u32,
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
    Paused,
    Canceled,
    Succeeded,
    Failed,
}

impl CrawlTaskStatus {
    fn as_db(&self) -> &'static str {
        match self {
            CrawlTaskStatus::Pending => "PENDING",
            CrawlTaskStatus::Running => "RUNNING",
            CrawlTaskStatus::Paused => "PAUSED",
            CrawlTaskStatus::Canceled => "CANCELED",
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
    Lagou,
    Manual,
}

impl SourceType {
    fn as_db(&self) -> &'static str {
        match self {
            SourceType::Boss => "boss",
            SourceType::Zhilian => "zhilian",
            SourceType::Wuba => "wuba",
            SourceType::Lagou => "lagou",
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum AiProvider {
    Qwen,
    Doubao,
    Deepseek,
    Minimax,
    Glm,
    OpenApi,
}

impl AiProvider {
    fn all() -> [AiProvider; 6] {
        [
            AiProvider::Qwen,
            AiProvider::Doubao,
            AiProvider::Deepseek,
            AiProvider::Minimax,
            AiProvider::Glm,
            AiProvider::OpenApi,
        ]
    }

    fn as_db(&self) -> &'static str {
        match self {
            AiProvider::Qwen => "qwen",
            AiProvider::Doubao => "doubao",
            AiProvider::Deepseek => "deepseek",
            AiProvider::Minimax => "minimax",
            AiProvider::Glm => "glm",
            AiProvider::OpenApi => "openapi",
        }
    }

    fn from_db(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "qwen" => AiProvider::Qwen,
            "doubao" => AiProvider::Doubao,
            "deepseek" => AiProvider::Deepseek,
            "minimax" => AiProvider::Minimax,
            "glm" => AiProvider::Glm,
            "openapi"
            | "open-api"
            | "openapi_compatible"
            | "openapi-compatible"
            | "openai_compatible"
            | "openai-compatible"
            | "openai" => AiProvider::OpenApi,
            "mock" => AiProvider::Qwen,
            _ => AiProvider::Qwen,
        }
    }

    fn default_model(&self) -> &'static str {
        match self {
            AiProvider::Qwen => "qwen-plus-latest",
            AiProvider::Doubao => "doubao-seed-1-6-250615",
            AiProvider::Deepseek => "deepseek-chat",
            AiProvider::Minimax => "MiniMax-M2.5",
            AiProvider::Glm => "glm-5-air",
            AiProvider::OpenApi => "gpt-4.1-mini",
        }
    }

    fn default_base_url(&self) -> &'static str {
        match self {
            AiProvider::Qwen => "https://dashscope.aliyuncs.com/compatible-mode/v1",
            AiProvider::Doubao => "https://ark.cn-beijing.volces.com/api/v3",
            AiProvider::Deepseek => "https://api.deepseek.com",
            AiProvider::Minimax => "https://api.minimaxi.com/v1",
            AiProvider::Glm => "https://open.bigmodel.cn/api/paas/v4",
            AiProvider::OpenApi => "https://api.openai.com/v1",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            AiProvider::Qwen => "千问 Qwen",
            AiProvider::Doubao => "豆包 Doubao",
            AiProvider::Deepseek => "DeepSeek",
            AiProvider::Minimax => "MiniMax",
            AiProvider::Glm => "GLM",
            AiProvider::OpenApi => "OpenApi",
        }
    }

    fn models(&self) -> &'static [&'static str] {
        match self {
            AiProvider::Qwen => &[
                "qwen3-max-preview",
                "qwen3-max-preview-thinking",
                "qwen3-max",
                "qwen-plus-latest",
                "qwen-plus",
                "qwen-turbo-latest",
                "qwen-turbo",
                "qwen-flash-latest",
                "qwen-flash",
                "qwen-long",
            ],
            AiProvider::Doubao => &[
                "doubao-seed-1-6-250615",
                "doubao-seed-1-6-thinking-250715",
                "doubao-seed-1-6-flash-250715",
            ],
            AiProvider::Deepseek => &["deepseek-chat", "deepseek-reasoner"],
            AiProvider::Minimax => &[
                "MiniMax-M2.5",
                "MiniMax-M2.5-Preview",
                "MiniMax-M2.5-Flash",
                "MiniMax-M2.5-highspeed",
                "MiniMax-M2.1",
                "abab8.5-chat",
                "abab8.5s-chat",
            ],
            AiProvider::Glm => &[
                "glm-5",
                "glm-5-air",
                "glm-5-airx",
                "glm-5-flash",
                "glm-4.5",
                "glm-4.5-air",
            ],
            AiProvider::OpenApi => &[
                "gpt-5",
                "gpt-5-mini",
                "gpt-5-nano",
                "gpt-4.1",
                "gpt-4.1-mini",
                "gpt-4.1-nano",
                "o4-mini",
            ],
        }
    }

    fn docs(&self) -> &'static [&'static str] {
        match self {
            AiProvider::Qwen => &[
                "https://help.aliyun.com/zh/model-studio/developer-reference/compatibility-of-openai-with-dashscope",
                "https://help.aliyun.com/zh/model-studio/getting-started/models",
            ],
            AiProvider::Doubao => &[
                "https://www.volcengine.com/docs/82379/1541594",
                "https://www.volcengine.com/docs/63993/1573666",
            ],
            AiProvider::Deepseek => &[
                "https://api-docs.deepseek.com/",
                "https://api-docs.deepseek.com/quick_start/pricing",
            ],
            AiProvider::Minimax => &[
                "https://platform.minimaxi.com/document/Quick%20Start",
                "https://platform.minimaxi.com/document/Compatibility%20with%20OpenAI",
            ],
            AiProvider::Glm => &[
                "https://docs.bigmodel.cn/cn/guide/models/text/glm-5",
                "https://docs.bigmodel.cn/cn/guide/models",
            ],
            AiProvider::OpenApi => &[
                "https://platform.openai.com/docs/models",
                "https://platform.openai.com/docs/api-reference/chat/create",
            ],
        }
    }

    fn to_catalog_item(&self) -> AiProviderCatalogItem {
        AiProviderCatalogItem {
            id: self.as_db().to_string(),
            label: self.label().to_string(),
            default_model: self.default_model().to_string(),
            default_base_url: self.default_base_url().to_string(),
            models: self.models().iter().map(|item| (*item).to_string()).collect(),
            docs: self.docs().iter().map(|item| (*item).to_string()).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct AiProviderCatalogItem {
    id: String,
    label: String,
    default_model: String,
    default_base_url: String,
    models: Vec<String>,
    docs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AiProviderCatalogView {
    providers: Vec<AiProviderCatalogItem>,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredAiProviderSettings {
    provider: String,
    model: String,
    base_url: String,
    api_key_enc: Option<String>,
    temperature: f64,
    max_tokens: i32,
    timeout_secs: i32,
    retry_count: i32,
}

impl StoredAiProviderSettings {
    fn defaults(provider: AiProvider) -> Self {
        Self {
            provider: provider.as_db().to_string(),
            model: provider.default_model().to_string(),
            base_url: provider.default_base_url().to_string(),
            api_key_enc: None,
            temperature: 0.2,
            max_tokens: 1500,
            timeout_secs: 35,
            retry_count: 2,
        }
    }
}

#[derive(Debug, Clone)]
struct ResolvedAiProviderSettings {
    provider: AiProvider,
    model: String,
    base_url: String,
    api_key: Option<String>,
    temperature: f64,
    max_tokens: i32,
    timeout_secs: i32,
    retry_count: i32,
}

#[derive(Debug, Clone, Serialize)]
struct AiProviderSettingsView {
    provider: String,
    model: String,
    base_url: String,
    temperature: f64,
    max_tokens: i32,
    timeout_secs: i32,
    retry_count: i32,
    has_api_key: bool,
}

#[derive(Debug, Clone, Serialize)]
struct AiProviderTestResult {
    ok: bool,
    provider: String,
    model: String,
    endpoint: String,
    latency_ms: u64,
    reply_excerpt: String,
    tested_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct UpsertAiProviderSettingsInput {
    provider: String,
    model: Option<String>,
    base_url: Option<String>,
    temperature: Option<f64>,
    max_tokens: Option<i32>,
    timeout_secs: Option<i32>,
    retry_count: Option<i32>,
    api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredAiProviderProfile {
    id: String,
    name: String,
    provider: String,
    model: String,
    base_url: String,
    api_key_enc: Option<String>,
    temperature: f64,
    max_tokens: i32,
    timeout_secs: i32,
    retry_count: i32,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredAiProviderProfiles {
    active_profile_id: String,
    profiles: Vec<StoredAiProviderProfile>,
}

#[derive(Debug, Clone, Serialize)]
struct AiProviderProfileView {
    id: String,
    name: String,
    provider: String,
    model: String,
    base_url: String,
    temperature: f64,
    max_tokens: i32,
    timeout_secs: i32,
    retry_count: i32,
    has_api_key: bool,
    is_active: bool,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct UpsertAiProviderProfileInput {
    profile_id: Option<String>,
    name: Option<String>,
    provider: String,
    model: Option<String>,
    base_url: Option<String>,
    temperature: Option<f64>,
    max_tokens: Option<i32>,
    timeout_secs: Option<i32>,
    retry_count: Option<i32>,
    api_key: Option<String>,
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

#[derive(Debug, Clone, Deserialize)]
struct MergeCandidateImportInput {
    candidate_id: i64,
    current_company: Option<String>,
    years_of_experience: Option<f64>,
    phone: Option<String>,
    email: Option<String>,
    tags: Option<Vec<String>>,
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

#[derive(Debug, Clone, Deserialize)]
struct ParseResumeFileInput {
    file_name: String,
    content_base64: String,
    enable_ocr: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct ParseResumeFileOutput {
    raw_text: String,
    parsed: Value,
    metadata: Value,
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

#[derive(Debug, Clone)]
struct AiPromptContext {
    required_skills: Vec<String>,
    extracted_skills: Vec<String>,
    candidate_years: f64,
    expected_salary_k: Option<f64>,
    max_salary_k: Option<f64>,
    stage: String,
    tags: Vec<String>,
    resume_raw_text: String,
    resume_parsed: Value,
}

#[derive(Debug, Clone)]
struct AiAnalysisPayload {
    overall_score: i32,
    dimension_scores: Vec<DimensionScore>,
    risks: Vec<String>,
    highlights: Vec<String>,
    suggestions: Vec<String>,
    evidence: Vec<EvidenceItem>,
    confidence: Option<f64>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScreeningDimension {
    key: String,
    label: String,
    weight: i32,
}

#[derive(Debug, Clone, Serialize)]
struct ScreeningTemplateRecord {
    id: i64,
    scope: String,
    name: String,
    job_id: Option<i64>,
    dimensions: Vec<ScreeningDimension>,
    risk_rules: Value,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct UpsertScreeningTemplateInput {
    job_id: Option<i64>,
    name: Option<String>,
    dimensions: Option<Vec<ScreeningDimension>>,
    risk_rules: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct RunScreeningInput {
    candidate_id: i64,
    job_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct ScreeningResultRecord {
    id: i64,
    candidate_id: i64,
    job_id: Option<i64>,
    template_id: Option<i64>,
    t0_score: f64,
    t1_score: i32,
    fine_score: i32,
    bonus_score: i32,
    risk_penalty: i32,
    overall_score: i32,
    recommendation: String,
    risk_level: String,
    evidence: Vec<String>,
    verification_points: Vec<String>,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InterviewQuestion {
    primary_question: String,
    follow_ups: Vec<String>,
    scoring_points: Vec<String>,
    red_flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct InterviewKitRecord {
    id: Option<i64>,
    candidate_id: i64,
    job_id: Option<i64>,
    questions: Vec<InterviewQuestion>,
    generated_by: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GenerateInterviewKitInput {
    candidate_id: i64,
    job_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct SaveInterviewKitInput {
    candidate_id: i64,
    job_id: Option<i64>,
    questions: Vec<InterviewQuestion>,
}

#[derive(Debug, Clone, Serialize)]
struct InterviewFeedbackRecord {
    id: i64,
    candidate_id: i64,
    job_id: Option<i64>,
    transcript_text: String,
    structured_feedback: Value,
    recording_path: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SubmitInterviewFeedbackInput {
    candidate_id: i64,
    job_id: Option<i64>,
    transcript_text: String,
    structured_feedback: Value,
    recording_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RunInterviewEvaluationInput {
    candidate_id: i64,
    job_id: Option<i64>,
    feedback_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct InterviewEvaluationRecord {
    id: i64,
    candidate_id: i64,
    job_id: Option<i64>,
    feedback_id: i64,
    recommendation: String,
    overall_score: i32,
    confidence: f64,
    evidence: Vec<String>,
    verification_points: Vec<String>,
    uncertainty: String,
    created_at: String,
}

#[derive(Debug, Clone)]
struct InterviewEvaluationPayload {
    recommendation: String,
    overall_score: i32,
    confidence: f64,
    evidence: Vec<String>,
    verification_points: Vec<String>,
    uncertainty: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskRuntimeSettings {
    auto_batch_concurrency: i32,
    auto_retry_count: i32,
    auto_retry_backoff_ms: i32,
}

#[derive(Debug, Clone, Deserialize)]
struct UpsertTaskRuntimeSettingsInput {
    auto_batch_concurrency: Option<i32>,
    auto_retry_count: Option<i32>,
    auto_retry_backoff_ms: Option<i32>,
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

fn sidecar_base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

fn sidecar_port_candidates(preferred_port: u16) -> Vec<u16> {
    let mut ports = vec![preferred_port];
    for offset in 1..=5_u16 {
        if let Some(port) = preferred_port.checked_add(offset) {
            ports.push(port);
        }
    }
    ports
}

fn sidecar_port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn sidecar_health_ok(port: u16) -> bool {
    let endpoint = format!("{}/health", sidecar_base_url(port));
    let client = match Client::builder().timeout(Duration::from_millis(850)).build() {
        Ok(client) => client,
        Err(_) => return false,
    };

    let response = match client.get(endpoint).send() {
        Ok(response) => response,
        Err(_) => return false,
    };
    if !response.status().is_success() {
        return false;
    }

    let payload = match response.json::<Value>() {
        Ok(payload) => payload,
        Err(_) => return false,
    };

    payload.get("ok").and_then(|value| value.as_bool()) == Some(true)
        && payload.get("service").and_then(|value| value.as_str()) == Some("crawler-sidecar")
}

fn sidecar_wait_until_healthy(port: u16, attempts: usize, delay_ms: u64) -> bool {
    for attempt in 0..attempts {
        if sidecar_health_ok(port) {
            return true;
        }

        if attempt + 1 < attempts {
            std::thread::sleep(Duration::from_millis(delay_ms));
        }
    }

    false
}

fn sidecar_spawn_process(manager: &SidecarManager, port: u16) -> Result<Child, String> {
    let mut command = if cfg!(target_os = "windows") {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg(&manager.command);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.arg("-lc").arg(&manager.command);
        cmd
    };

    command
        .current_dir(&manager.cwd)
        .env("CRAWLER_PORT", port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    command.spawn().map_err(|error| error.to_string())
}

fn sidecar_stop_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn ensure_sidecar_running(state: &AppState) -> Result<SidecarRuntime, String> {
    let mut manager = state
        .sidecar
        .lock()
        .map_err(|_| "sidecar_lock_poisoned".to_string())?;

    let mut exited_recently = false;
    if let Some(child) = manager.child.as_mut() {
        match child.try_wait() {
            Ok(Some(status)) => {
                manager.last_error = Some(format!("sidecar_exit_status_{status}"));
                manager.child = None;
                exited_recently = true;
            }
            Ok(None) => {}
            Err(error) => {
                manager.last_error = Some(error.to_string());
                manager.child = None;
                exited_recently = true;
            }
        }
    }

    let mut probe_ports = vec![manager.active_port];
    for port in sidecar_port_candidates(manager.preferred_port) {
        if !probe_ports.contains(&port) {
            probe_ports.push(port);
        }
    }

    for port in probe_ports {
        if sidecar_health_ok(port) {
            manager.active_port = port;
            return Ok(SidecarRuntime {
                ok: true,
                port,
                base_url: sidecar_base_url(port),
                source: if exited_recently {
                    "recovered_existing".to_string()
                } else {
                    "existing".to_string()
                },
                message: manager.last_error.clone(),
                restart_count: manager.restart_count,
            });
        }
    }

    let mut any_port_available = false;
    let mut last_spawn_error = manager.last_error.clone().unwrap_or_default();
    let mut spawned_at_least_once = false;

    for port in sidecar_port_candidates(manager.preferred_port) {
        if !sidecar_port_available(port) {
            continue;
        }
        any_port_available = true;

        let mut child = match sidecar_spawn_process(&manager, port) {
            Ok(child) => child,
            Err(error) => {
                last_spawn_error = error;
                continue;
            }
        };
        spawned_at_least_once = true;

        if sidecar_wait_until_healthy(port, 9, 320) {
            manager.active_port = port;
            manager.restart_count = manager.restart_count.saturating_add(1);
            manager.last_error = None;
            manager.child = Some(child);

            return Ok(SidecarRuntime {
                ok: true,
                port,
                base_url: sidecar_base_url(port),
                source: if exited_recently { "restarted" } else { "spawned" }.to_string(),
                message: None,
                restart_count: manager.restart_count,
            });
        }

        sidecar_stop_child(&mut child);
    }

    if !any_port_available {
        manager.last_error = Some("sidecar_port_conflict".to_string());
        return Err("sidecar_port_conflict".to_string());
    }

    if spawned_at_least_once {
        manager.last_error = Some("sidecar_start_timeout".to_string());
        return Err("sidecar_start_timeout".to_string());
    }

    let error_text = if last_spawn_error.trim().is_empty() {
        "sidecar_start_failed".to_string()
    } else {
        last_spawn_error
    };
    manager.last_error = Some(error_text.clone());
    Err(error_text)
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

fn merge_candidate_tags(existing: &[String], incoming: &[String]) -> Vec<String> {
    let mut seen = BTreeMap::<String, bool>::new();
    let mut merged = Vec::<String>::new();

    for raw in existing.iter().chain(incoming.iter()) {
        let text = raw.trim();
        if text.is_empty() {
            continue;
        }

        let key = text.to_lowercase();
        if seen.insert(key, true).is_none() {
            merged.push(text.to_string());
        }
    }

    merged
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

        CREATE TABLE IF NOT EXISTS screening_templates (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            scope TEXT NOT NULL,
            job_id INTEGER,
            name TEXT NOT NULL,
            config_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(scope, job_id),
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS screening_dimensions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            template_id INTEGER NOT NULL,
            dimension_key TEXT NOT NULL,
            dimension_label TEXT NOT NULL,
            weight INTEGER NOT NULL,
            sort_order INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(template_id, dimension_key),
            FOREIGN KEY(template_id) REFERENCES screening_templates(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS job_screening_overrides (
            job_id INTEGER PRIMARY KEY,
            template_id INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE CASCADE,
            FOREIGN KEY(template_id) REFERENCES screening_templates(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS screening_results (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            candidate_id INTEGER NOT NULL,
            job_id INTEGER,
            template_id INTEGER,
            t0_score REAL NOT NULL,
            t1_score INTEGER NOT NULL,
            fine_score INTEGER NOT NULL,
            bonus_score INTEGER NOT NULL,
            risk_penalty INTEGER NOT NULL,
            overall_score INTEGER NOT NULL,
            recommendation TEXT NOT NULL,
            risk_level TEXT NOT NULL,
            evidence_json TEXT NOT NULL,
            verification_points_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(candidate_id) REFERENCES candidates(id) ON DELETE CASCADE,
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE SET NULL,
            FOREIGN KEY(template_id) REFERENCES screening_templates(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS interview_kits (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            slot_key TEXT NOT NULL UNIQUE,
            candidate_id INTEGER NOT NULL,
            job_id INTEGER,
            content_json TEXT NOT NULL,
            generated_by TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(candidate_id) REFERENCES candidates(id) ON DELETE CASCADE,
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS interview_feedback (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            candidate_id INTEGER NOT NULL,
            job_id INTEGER,
            transcript_text TEXT NOT NULL,
            structured_feedback_json TEXT NOT NULL,
            recording_path TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(candidate_id) REFERENCES candidates(id) ON DELETE CASCADE,
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS interview_evaluations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            candidate_id INTEGER NOT NULL,
            job_id INTEGER,
            feedback_id INTEGER NOT NULL,
            recommendation TEXT NOT NULL,
            overall_score INTEGER NOT NULL,
            confidence REAL NOT NULL,
            evidence_json TEXT NOT NULL,
            verification_points_json TEXT NOT NULL,
            uncertainty TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(candidate_id) REFERENCES candidates(id) ON DELETE CASCADE,
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE SET NULL,
            FOREIGN KEY(feedback_id) REFERENCES interview_feedback(id) ON DELETE CASCADE
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

        CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_jobs_updated_at ON jobs(updated_at);
        CREATE INDEX IF NOT EXISTS idx_candidates_updated_at ON candidates(updated_at);
        CREATE INDEX IF NOT EXISTS idx_candidates_stage ON candidates(stage);
        CREATE INDEX IF NOT EXISTS idx_applications_job_stage ON applications(job_id, stage);
        CREATE INDEX IF NOT EXISTS idx_crawl_tasks_status ON crawl_tasks(status);
        CREATE INDEX IF NOT EXISTS idx_screening_results_candidate ON screening_results(candidate_id, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_interview_kits_candidate ON interview_kits(candidate_id, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_interview_feedback_candidate ON interview_feedback(candidate_id, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_interview_evaluations_candidate ON interview_evaluations(candidate_id, created_at DESC);

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

const AI_SETTINGS_KEY: &str = "ai_provider_settings_v1";
const AI_SETTINGS_PROFILES_KEY: &str = "ai_provider_profiles_v1";
const TASK_RUNTIME_SETTINGS_KEY: &str = "task_runtime_settings_v1";

fn default_task_runtime_settings() -> TaskRuntimeSettings {
    TaskRuntimeSettings {
        auto_batch_concurrency: 2,
        auto_retry_count: 1,
        auto_retry_backoff_ms: 450,
    }
}

fn normalize_task_runtime_settings(input: TaskRuntimeSettings) -> TaskRuntimeSettings {
    TaskRuntimeSettings {
        auto_batch_concurrency: input.auto_batch_concurrency.clamp(1, 8),
        auto_retry_count: input.auto_retry_count.clamp(0, 6),
        auto_retry_backoff_ms: input.auto_retry_backoff_ms.clamp(100, 8_000),
    }
}

fn read_task_runtime_settings(conn: &Connection) -> AppResult<TaskRuntimeSettings> {
    let maybe_text = conn
        .query_row(
            "SELECT value_json FROM app_settings WHERE key = ?1",
            [TASK_RUNTIME_SETTINGS_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    if let Some(text) = maybe_text {
        if let Ok(settings) = serde_json::from_str::<TaskRuntimeSettings>(&text) {
            return Ok(normalize_task_runtime_settings(settings));
        }
    }

    Ok(default_task_runtime_settings())
}

fn write_task_runtime_settings(conn: &Connection, settings: &TaskRuntimeSettings) -> AppResult<()> {
    let normalized = normalize_task_runtime_settings(settings.clone());
    let value = serde_json::to_string(&normalized)?;
    let now = now_iso();
    conn.execute(
        r#"
        INSERT INTO app_settings(key, value_json, updated_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET
            value_json = excluded.value_json,
            updated_at = excluded.updated_at
        "#,
        params![TASK_RUNTIME_SETTINGS_KEY, value, now],
    )?;
    Ok(())
}

fn extract_json_object_block(text: &str) -> Option<String> {
    let mut start: Option<usize> = None;
    let mut depth: i32 = 0;

    for (index, ch) in text.char_indices() {
        if ch == '{' {
            if start.is_none() {
                start = Some(index);
            }
            depth += 1;
            continue;
        }

        if ch == '}' && depth > 0 {
            depth -= 1;
            if depth == 0 {
                if let Some(start_index) = start {
                    return Some(text[start_index..=index].to_string());
                }
            }
        }
    }

    None
}

fn parse_json_from_text(text: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Ok(value);
    }

    let extracted = extract_json_object_block(trimmed)
        .ok_or_else(|| "provider_response_not_json".to_string())?;
    serde_json::from_str::<Value>(&extracted).map_err(|error| error.to_string())
}

fn get_array_strings(value: &Value, snake: &str, camel: &str) -> Vec<String> {
    value
        .get(snake)
        .or_else(|| value.get(camel))
        .and_then(|item| item.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(|text| text.trim().to_string()))
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn get_i32(value: &Value, snake: &str, camel: &str) -> Option<i32> {
    value
        .get(snake)
        .or_else(|| value.get(camel))
        .and_then(|item| item.as_i64())
        .map(|number| number as i32)
}

fn get_f64(value: &Value, snake: &str, camel: &str) -> Option<f64> {
    value
        .get(snake)
        .or_else(|| value.get(camel))
        .and_then(|item| item.as_f64())
}

fn parse_dimension_scores(value: &Value) -> Vec<DimensionScore> {
    let rows = value
        .get("dimension_scores")
        .or_else(|| value.get("dimensionScores"))
        .and_then(|item| item.as_array())
        .cloned()
        .unwrap_or_default();

    rows.iter()
        .filter_map(|item| {
            let key = item.get("key").and_then(|field| field.as_str())?.trim().to_string();
            if key.is_empty() {
                return None;
            }

            let score = item
                .get("score")
                .and_then(|field| field.as_i64())
                .map(|field| field as i32)
                .unwrap_or(70);
            let reason = item
                .get("reason")
                .and_then(|field| field.as_str())
                .map(|field| field.trim().to_string())
                .filter(|field| !field.is_empty())
                .unwrap_or_else(|| "模型未给出充分理由，采用默认说明。".to_string());

            Some(DimensionScore {
                key,
                score: clamp_score(score),
                reason,
            })
        })
        .collect()
}

fn parse_evidence(value: &Value) -> Vec<EvidenceItem> {
    let rows = value
        .get("evidence")
        .and_then(|item| item.as_array())
        .cloned()
        .unwrap_or_default();

    rows.iter()
        .filter_map(|item| {
            let dimension = item
                .get("dimension")
                .and_then(|field| field.as_str())
                .map(|field| field.trim().to_string())
                .filter(|field| !field.is_empty())?;
            let statement = item
                .get("statement")
                .and_then(|field| field.as_str())
                .map(|field| field.trim().to_string())
                .filter(|field| !field.is_empty())?;
            let source_snippet = item
                .get("source_snippet")
                .or_else(|| item.get("sourceSnippet"))
                .and_then(|field| field.as_str())
                .map(|field| field.trim().to_string())
                .filter(|field| !field.is_empty())
                .unwrap_or_else(|| "模型未返回证据片段。".to_string());

            Some(EvidenceItem {
                dimension,
                statement,
                source_snippet,
            })
        })
        .collect()
}

fn parse_ai_provider_response(text: &str) -> Result<AiAnalysisPayload, String> {
    let value = parse_json_from_text(text)?;
    let overall_score = get_i32(&value, "overall_score", "overallScore")
        .map(clamp_score)
        .ok_or_else(|| "provider_overall_score_missing".to_string())?;

    let dimension_scores = parse_dimension_scores(&value);
    if dimension_scores.is_empty() {
        return Err("provider_dimension_scores_empty".to_string());
    }

    let risks = get_array_strings(&value, "risks", "risks");
    let highlights = get_array_strings(&value, "highlights", "highlights");
    let suggestions = get_array_strings(&value, "suggestions", "suggestions");
    let evidence = parse_evidence(&value);
    let confidence = get_f64(&value, "confidence", "confidence");

    Ok(AiAnalysisPayload {
        overall_score,
        dimension_scores,
        risks,
        highlights,
        suggestions,
        evidence,
        confidence,
    })
}

fn ensure_analysis_payload(
    payload: AiAnalysisPayload,
    fallback: &AiAnalysisPayload,
) -> AiAnalysisPayload {
    let mut dimensions = BTreeMap::<String, DimensionScore>::new();
    for score in payload.dimension_scores {
        dimensions.insert(
            score.key.clone(),
            DimensionScore {
                key: score.key,
                score: clamp_score(score.score),
                reason: score.reason,
            },
        );
    }

    for score in &fallback.dimension_scores {
        dimensions.entry(score.key.clone()).or_insert_with(|| score.clone());
    }

    let ordered_keys = ["skill_match", "experience", "compensation", "stability"];
    let dimension_scores = ordered_keys
        .iter()
        .filter_map(|key| dimensions.remove(*key))
        .collect::<Vec<_>>();

    let weighted_score = clamp_score(
        (dimension_scores
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let weight = match index {
                    0 => 0.4,
                    1 => 0.25,
                    2 => 0.15,
                    _ => 0.2,
                };
                item.score as f64 * weight
            })
            .sum::<f64>())
            .round() as i32,
    );

    let overall_score = if payload.overall_score > 0 {
        clamp_score(payload.overall_score)
    } else {
        weighted_score
    };

    AiAnalysisPayload {
        overall_score,
        dimension_scores,
        risks: if payload.risks.is_empty() {
            fallback.risks.clone()
        } else {
            payload.risks
        },
        highlights: if payload.highlights.is_empty() {
            fallback.highlights.clone()
        } else {
            payload.highlights
        },
        suggestions: if payload.suggestions.is_empty() {
            fallback.suggestions.clone()
        } else {
            payload.suggestions
        },
        evidence: if payload.evidence.is_empty() {
            fallback.evidence.clone()
        } else {
            payload.evidence
        },
        confidence: payload.confidence,
    }
}

fn make_ai_profile_id() -> String {
    format!(
        "profile-{}-{:08x}",
        Utc::now().timestamp_millis(),
        rand::random::<u32>()
    )
}

fn profile_default_name(provider: AiProvider, ordinal: usize) -> String {
    if ordinal <= 1 {
        format!("{} 默认", provider.label())
    } else {
        format!("{} 配置{}", provider.label(), ordinal)
    }
}

fn profile_to_stored_settings(profile: &StoredAiProviderProfile) -> StoredAiProviderSettings {
    StoredAiProviderSettings {
        provider: profile.provider.clone(),
        model: profile.model.clone(),
        base_url: profile.base_url.clone(),
        api_key_enc: profile.api_key_enc.clone(),
        temperature: profile.temperature,
        max_tokens: profile.max_tokens,
        timeout_secs: profile.timeout_secs,
        retry_count: profile.retry_count,
    }
}

fn build_profile_from_settings(
    settings: &StoredAiProviderSettings,
    name: String,
    created_at: String,
) -> StoredAiProviderProfile {
    let provider = AiProvider::from_db(&settings.provider);
    let provider_raw = settings.provider.trim().to_lowercase();
    let legacy_mock_provider = provider_raw == "mock";

    let model = {
        let text = settings.model.trim();
        if legacy_mock_provider || text.is_empty() {
            provider.default_model().to_string()
        } else {
            text.to_string()
        }
    };

    let base_url = {
        let text = settings.base_url.trim().trim_end_matches('/');
        if legacy_mock_provider || text.is_empty() {
            provider.default_base_url().to_string()
        } else {
            text.to_string()
        }
    };

    StoredAiProviderProfile {
        id: make_ai_profile_id(),
        name,
        provider: provider.as_db().to_string(),
        model,
        base_url,
        api_key_enc: settings.api_key_enc.clone(),
        temperature: settings.temperature.clamp(0.0, 1.2),
        max_tokens: settings.max_tokens.clamp(200, 8192),
        timeout_secs: settings.timeout_secs.clamp(8, 180),
        retry_count: settings.retry_count.clamp(1, 5),
        created_at: created_at.clone(),
        updated_at: created_at,
    }
}

fn normalize_profile_in_place(profile: &mut StoredAiProviderProfile, ordinal: usize) {
    let provider = AiProvider::from_db(&profile.provider);
    profile.provider = provider.as_db().to_string();

    let model = profile.model.trim();
    profile.model = if model.is_empty() {
        provider.default_model().to_string()
    } else {
        model.to_string()
    };

    let base_url = profile.base_url.trim().trim_end_matches('/');
    profile.base_url = if base_url.is_empty() {
        provider.default_base_url().to_string()
    } else {
        base_url.to_string()
    };

    let name = profile.name.trim();
    profile.name = if name.is_empty() {
        profile_default_name(provider, ordinal)
    } else {
        name.to_string()
    };

    profile.temperature = profile.temperature.clamp(0.0, 1.2);
    profile.max_tokens = profile.max_tokens.clamp(200, 8192);
    profile.timeout_secs = profile.timeout_secs.clamp(8, 180);
    profile.retry_count = profile.retry_count.clamp(1, 5);

    if profile.id.trim().is_empty() {
        profile.id = make_ai_profile_id();
    }
    if profile.created_at.trim().is_empty() {
        profile.created_at = now_iso();
    }
    if profile.updated_at.trim().is_empty() {
        profile.updated_at = profile.created_at.clone();
    }
}

fn active_profile<'a>(profiles: &'a StoredAiProviderProfiles) -> Option<&'a StoredAiProviderProfile> {
    profiles
        .profiles
        .iter()
        .find(|item| item.id == profiles.active_profile_id)
}

fn read_legacy_ai_settings(conn: &Connection) -> AppResult<StoredAiProviderSettings> {
    let maybe_text = conn
        .query_row(
            "SELECT value_json FROM app_settings WHERE key = ?1",
            [AI_SETTINGS_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    if let Some(text) = maybe_text {
        if let Ok(settings) = serde_json::from_str::<StoredAiProviderSettings>(&text) {
            return Ok(settings);
        }
    }

    Ok(StoredAiProviderSettings::defaults(AiProvider::Qwen))
}

fn write_legacy_ai_settings(conn: &Connection, settings: &StoredAiProviderSettings) -> AppResult<()> {
    let value = serde_json::to_string(settings)?;
    let now = now_iso();
    conn.execute(
        r#"
        INSERT INTO app_settings(key, value_json, updated_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET
            value_json = excluded.value_json,
            updated_at = excluded.updated_at
        "#,
        params![AI_SETTINGS_KEY, value, now],
    )?;
    Ok(())
}

fn read_ai_profiles(conn: &Connection) -> AppResult<StoredAiProviderProfiles> {
    let maybe_text = conn
        .query_row(
            "SELECT value_json FROM app_settings WHERE key = ?1",
            [AI_SETTINGS_PROFILES_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    if let Some(text) = maybe_text {
        if let Ok(mut state) = serde_json::from_str::<StoredAiProviderProfiles>(&text) {
            state.profiles.retain(|item| !item.id.trim().is_empty());
            for (index, profile) in state.profiles.iter_mut().enumerate() {
                normalize_profile_in_place(profile, index + 1);
            }
            if !state.profiles.is_empty() {
                if !state
                    .profiles
                    .iter()
                    .any(|item| item.id == state.active_profile_id)
                {
                    state.active_profile_id = state.profiles[0].id.clone();
                }
                return Ok(state);
            }
        }
    }

    let legacy = read_legacy_ai_settings(conn)?;
    let fallback_provider = AiProvider::from_db(&legacy.provider);
    let fallback_name = profile_default_name(fallback_provider, 1);
    let created_at = now_iso();
    let profile = build_profile_from_settings(&legacy, fallback_name, created_at);
    Ok(StoredAiProviderProfiles {
        active_profile_id: profile.id.clone(),
        profiles: vec![profile],
    })
}

fn write_ai_profiles(conn: &Connection, state: &StoredAiProviderProfiles) -> AppResult<()> {
    let value = serde_json::to_string(state)?;
    let now = now_iso();
    conn.execute(
        r#"
        INSERT INTO app_settings(key, value_json, updated_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET
            value_json = excluded.value_json,
            updated_at = excluded.updated_at
        "#,
        params![AI_SETTINGS_PROFILES_KEY, value, now],
    )?;

    if let Some(active) = active_profile(state) {
        write_legacy_ai_settings(conn, &profile_to_stored_settings(active))?;
    }

    Ok(())
}

fn read_ai_settings(conn: &Connection) -> AppResult<StoredAiProviderSettings> {
    let state = read_ai_profiles(conn)?;
    if let Some(active) = active_profile(&state) {
        return Ok(profile_to_stored_settings(active));
    }

    Ok(StoredAiProviderSettings::defaults(AiProvider::Qwen))
}

fn write_ai_settings(conn: &Connection, settings: &StoredAiProviderSettings) -> AppResult<()> {
    let mut state = read_ai_profiles(conn)?;
    let active_index = state
        .profiles
        .iter()
        .position(|item| item.id == state.active_profile_id)
        .unwrap_or(0);
    let now = now_iso();

    if let Some(active) = state.profiles.get_mut(active_index) {
        active.provider = settings.provider.trim().to_string();
        active.model = settings.model.trim().to_string();
        active.base_url = settings.base_url.trim().trim_end_matches('/').to_string();
        active.api_key_enc = settings.api_key_enc.clone();
        active.temperature = settings.temperature;
        active.max_tokens = settings.max_tokens;
        active.timeout_secs = settings.timeout_secs;
        active.retry_count = settings.retry_count;
        active.updated_at = now;
        normalize_profile_in_place(active, active_index + 1);
    } else {
        let provider = AiProvider::from_db(&settings.provider);
        let name = profile_default_name(provider, 1);
        let profile = build_profile_from_settings(settings, name, now.clone());
        state.active_profile_id = profile.id.clone();
        state.profiles = vec![profile];
    }

    write_ai_profiles(conn, &state)
}

fn to_ai_profile_views(state: &StoredAiProviderProfiles) -> Vec<AiProviderProfileView> {
    state
        .profiles
        .iter()
        .map(|profile| {
            let provider = AiProvider::from_db(&profile.provider);
            AiProviderProfileView {
                id: profile.id.clone(),
                name: profile.name.clone(),
                provider: provider.as_db().to_string(),
                model: profile.model.clone(),
                base_url: profile.base_url.clone(),
                temperature: profile.temperature,
                max_tokens: profile.max_tokens,
                timeout_secs: profile.timeout_secs,
                retry_count: profile.retry_count,
                has_api_key: profile.api_key_enc.is_some()
                    || provider_specific_api_key(&provider).is_some(),
                is_active: profile.id == state.active_profile_id,
                created_at: profile.created_at.clone(),
                updated_at: profile.updated_at.clone(),
            }
        })
        .collect()
}

fn provider_specific_api_key(provider: &AiProvider) -> Option<String> {
    let key_names: &[&str] = match provider {
        AiProvider::Qwen => &["DOSS_QWEN_API_KEY"],
        AiProvider::Doubao => &["DOSS_DOUBAO_API_KEY"],
        AiProvider::Deepseek => &["DOSS_DEEPSEEK_API_KEY"],
        AiProvider::Minimax => &["DOSS_MINIMAX_API_KEY"],
        AiProvider::Glm => &["DOSS_GLM_API_KEY"],
        AiProvider::OpenApi => {
            &[
                "DOSS_OPENAPI_API_KEY",
                "DOSS_OPENAI_COMPAT_API_KEY",
                "DOSS_OPENAI_API_KEY",
                "OPENAI_API_KEY",
            ]
        }
    };

    for key_name in key_names {
        if let Ok(value) = std::env::var(key_name) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    None
}

fn resolve_ai_settings(
    conn: &Connection,
    cipher: &FieldCipher,
) -> AppResult<ResolvedAiProviderSettings> {
    let stored = read_ai_settings(conn)?;
    resolve_ai_settings_from_stored(&stored, cipher)
}

fn resolve_ai_settings_from_stored(
    stored: &StoredAiProviderSettings,
    cipher: &FieldCipher,
) -> AppResult<ResolvedAiProviderSettings> {
    let stored_provider_raw = stored.provider.trim().to_lowercase();
    let legacy_mock_provider = stored_provider_raw == "mock";

    let provider = std::env::var("DOSS_AI_PROVIDER")
        .ok()
        .map(|value| AiProvider::from_db(&value))
        .unwrap_or_else(|| AiProvider::from_db(&stored.provider));

    let decrypted_api_key = stored
        .api_key_enc
        .as_deref()
        .map(|value| cipher.decrypt(value))
        .transpose()?;

    let model = std::env::var("DOSS_AI_MODEL")
        .ok()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .unwrap_or_else(|| {
            let trimmed = stored.model.trim();
            if legacy_mock_provider || trimmed.is_empty() {
                provider.default_model().to_string()
            } else {
                trimmed.to_string()
            }
        });

    let base_url = std::env::var("DOSS_AI_BASE_URL")
        .ok()
        .map(|item| item.trim().trim_end_matches('/').to_string())
        .filter(|item| !item.is_empty())
        .unwrap_or_else(|| {
            let trimmed = stored.base_url.trim().trim_end_matches('/');
            if legacy_mock_provider || trimmed.is_empty() {
                provider.default_base_url().to_string()
            } else {
                trimmed.to_string()
            }
        });

    let api_key = std::env::var("DOSS_AI_API_KEY")
        .ok()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .or_else(|| provider_specific_api_key(&provider))
        .or(decrypted_api_key);

    let temperature = std::env::var("DOSS_AI_TEMPERATURE")
        .ok()
        .and_then(|item| item.parse::<f64>().ok())
        .unwrap_or(stored.temperature)
        .clamp(0.0, 1.2);
    let max_tokens = std::env::var("DOSS_AI_MAX_TOKENS")
        .ok()
        .and_then(|item| item.parse::<i32>().ok())
        .unwrap_or(stored.max_tokens)
        .clamp(200, 8192);
    let timeout_secs = std::env::var("DOSS_AI_TIMEOUT_SECS")
        .ok()
        .and_then(|item| item.parse::<i32>().ok())
        .unwrap_or(stored.timeout_secs)
        .clamp(8, 180);
    let retry_count = std::env::var("DOSS_AI_RETRY_COUNT")
        .ok()
        .and_then(|item| item.parse::<i32>().ok())
        .unwrap_or(stored.retry_count)
        .clamp(1, 5);

    Ok(ResolvedAiProviderSettings {
        provider,
        model,
        base_url,
        api_key,
        temperature,
        max_tokens,
        timeout_secs,
        retry_count,
    })
}

fn resolve_ai_settings_with_input_overrides(
    conn: &Connection,
    cipher: &FieldCipher,
    input: &UpsertAiProviderSettingsInput,
) -> AppResult<ResolvedAiProviderSettings> {
    let mut resolved = resolve_ai_settings(conn, cipher)?;
    let requested_provider = AiProvider::from_db(&input.provider);
    let provider_changed = requested_provider != resolved.provider;
    resolved.provider = requested_provider;

    if let Some(model) = input.model.as_deref().map(str::trim).filter(|item| !item.is_empty()) {
        resolved.model = model.to_string();
    } else if provider_changed || resolved.model.trim().is_empty() {
        resolved.model = resolved.provider.default_model().to_string();
    }

    if let Some(base_url) = input
        .base_url
        .as_deref()
        .map(|item| item.trim().trim_end_matches('/'))
        .filter(|item| !item.is_empty())
    {
        resolved.base_url = base_url.to_string();
    } else if provider_changed || resolved.base_url.trim().is_empty() {
        resolved.base_url = resolved.provider.default_base_url().to_string();
    }

    if let Some(api_key_raw) = input.api_key.as_deref() {
        let trimmed = api_key_raw.trim();
        resolved.api_key = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
    } else if provider_changed {
        resolved.api_key = provider_specific_api_key(&resolved.provider);
    }

    resolved.temperature = input
        .temperature
        .unwrap_or(resolved.temperature)
        .clamp(0.0, 1.2);
    resolved.max_tokens = input
        .max_tokens
        .unwrap_or(resolved.max_tokens)
        .clamp(200, 8192);
    resolved.timeout_secs = input
        .timeout_secs
        .unwrap_or(resolved.timeout_secs)
        .clamp(8, 180);
    resolved.retry_count = input
        .retry_count
        .unwrap_or(resolved.retry_count)
        .clamp(1, 5);

    Ok(resolved)
}

fn resolve_ai_settings_for_profile(
    conn: &Connection,
    cipher: &FieldCipher,
    profile_id: &str,
) -> AppResult<ResolvedAiProviderSettings> {
    let state = read_ai_profiles(conn)?;
    let profile = state
        .profiles
        .iter()
        .find(|item| item.id == profile_id)
        .ok_or_else(|| AppError::NotFound(format!("AI profile {profile_id} not found")))?;
    let stored = profile_to_stored_settings(profile);
    resolve_ai_settings_from_stored(&stored, cipher)
}

fn to_ai_settings_view(settings: &ResolvedAiProviderSettings) -> AiProviderSettingsView {
    AiProviderSettingsView {
        provider: settings.provider.as_db().to_string(),
        model: settings.model.clone(),
        base_url: settings.base_url.clone(),
        temperature: settings.temperature,
        max_tokens: settings.max_tokens,
        timeout_secs: settings.timeout_secs,
        retry_count: settings.retry_count,
        has_api_key: settings.api_key.is_some(),
    }
}

fn trim_resume_excerpt(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

fn build_ai_prompts(context: &AiPromptContext) -> (String, String) {
    let system_prompt = r#"你是资深招聘顾问。请基于给定候选人资料，输出严格 JSON（不要 markdown），字段如下：
{
  "overall_score": 0-100 的整数,
  "dimension_scores": [
    {"key":"skill_match","score":0-100,"reason":"..."},
    {"key":"experience","score":0-100,"reason":"..."},
    {"key":"compensation","score":0-100,"reason":"..."},
    {"key":"stability","score":0-100,"reason":"..."}
  ],
  "risks": ["..."],
  "highlights": ["..."],
  "suggestions": ["..."],
  "evidence": [{"dimension":"...","statement":"...","source_snippet":"..."}],
  "confidence": 0-1
}
所有结论必须可解释，禁止编造不存在的信息。"#;

    let user_payload = serde_json::json!({
        "requiredSkills": context.required_skills,
        "candidateSkills": context.extracted_skills,
        "candidateYears": context.candidate_years,
        "expectedSalaryK": context.expected_salary_k,
        "jobMaxSalaryK": context.max_salary_k,
        "stage": context.stage,
        "tags": context.tags,
        "resumeParsed": context.resume_parsed,
        "resumeExcerpt": trim_resume_excerpt(&context.resume_raw_text, 2600),
    });

    (system_prompt.to_string(), user_payload.to_string())
}

fn ensure_openai_endpoint(base_url: &str) -> String {
    let normalized = base_url.trim().trim_end_matches('/');
    if normalized.ends_with("/chat/completions") {
        normalized.to_string()
    } else {
        format!("{normalized}/chat/completions")
    }
}

fn ensure_minimax_endpoint(base_url: &str) -> String {
    let normalized = base_url.trim().trim_end_matches('/');
    if normalized.ends_with("/text/chatcompletion_v2") {
        normalized.to_string()
    } else {
        format!("{normalized}/text/chatcompletion_v2")
    }
}

fn parse_openai_content(response: &Value) -> Option<String> {
    if let Some(content) = response
        .pointer("/choices/0/message/content")
        .and_then(|value| value.as_str())
    {
        return Some(content.to_string());
    }

    response
        .pointer("/choices/0/message/content")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("text").and_then(|value| value.as_str()))
                .collect::<Vec<_>>()
                .join("")
        })
        .filter(|value| !value.trim().is_empty())
}

fn parse_minimax_content(response: &Value) -> Result<String, String> {
    if let Some(status_code) = response
        .pointer("/base_resp/status_code")
        .and_then(|value| value.as_i64())
    {
        if status_code != 0 {
            let status_msg = response
                .pointer("/base_resp/status_msg")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("unknown_error");
            return Err(format!("provider_api_error_{}: {}", status_code, status_msg));
        }
    }

    if let Some(reply) = response
        .get("reply")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(reply.to_string());
    }

    if let Some(content) = parse_openai_content(response) {
        return Ok(content);
    }

    Err("provider_response_content_missing".to_string())
}

fn call_openai_compatible_provider(
    client: &Client,
    settings: &ResolvedAiProviderSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, String> {
    let endpoint = ensure_openai_endpoint(&settings.base_url);
    let api_key = settings
        .api_key
        .as_ref()
        .ok_or_else(|| "provider_api_key_missing".to_string())?;

    let response = client
        .post(endpoint)
        .bearer_auth(api_key)
        .json(&serde_json::json!({
            "model": settings.model,
            "temperature": settings.temperature,
            "max_tokens": settings.max_tokens,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ]
        }))
        .send()
        .map_err(|error| error.to_string())?;

    let status = response.status();
    let body_text = response.text().map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("provider_http_{}: {}", status.as_u16(), trim_resume_excerpt(&body_text, 300)));
    }

    let body_json = serde_json::from_str::<Value>(&body_text).map_err(|error| error.to_string())?;
    parse_openai_content(&body_json).ok_or_else(|| "provider_response_content_missing".to_string())
}

fn call_minimax_provider(
    client: &Client,
    settings: &ResolvedAiProviderSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, String> {
    let endpoint = ensure_minimax_endpoint(&settings.base_url);
    let api_key = settings
        .api_key
        .as_ref()
        .ok_or_else(|| "provider_api_key_missing".to_string())?;

    let response = client
        .post(endpoint)
        .bearer_auth(api_key)
        .json(&serde_json::json!({
            "model": settings.model,
            "temperature": settings.temperature,
            "max_tokens": settings.max_tokens,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ]
        }))
        .send()
        .map_err(|error| error.to_string())?;

    let status = response.status();
    let body_text = response.text().map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!(
            "provider_http_{}: {}",
            status.as_u16(),
            trim_resume_excerpt(&body_text, 300)
        ));
    }

    let body_json = serde_json::from_str::<Value>(&body_text).map_err(|error| error.to_string())?;
    parse_minimax_content(&body_json).map_err(|error| {
        if error == "provider_response_content_missing" {
            format!("{error}: {}", trim_resume_excerpt(&body_text, 300))
        } else {
            error
        }
    })
}

fn probe_provider_connectivity(
    settings: ResolvedAiProviderSettings,
) -> Result<(Result<String, String>, u64), String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(settings.timeout_secs as u64))
        .build()
        .map_err(|error| error.to_string())?;

    let start = Instant::now();
    let response = match settings.provider {
        AiProvider::Minimax => call_minimax_provider(
            &client,
            &settings,
            "You are a connectivity checker. Reply with exactly OK.",
            "ping",
        ),
        _ => call_openai_compatible_provider(
            &client,
            &settings,
            "You are a connectivity checker. Reply with exactly OK.",
            "ping",
        ),
    };
    let latency_ms = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
    Ok((response, latency_ms))
}

fn invoke_cloud_provider(
    settings: &ResolvedAiProviderSettings,
    context: &AiPromptContext,
    fallback: &AiAnalysisPayload,
) -> Result<AiAnalysisPayload, String> {
    if settings.api_key.is_none() {
        return Err(format!("{}_api_key_missing", settings.provider.as_db()));
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(settings.timeout_secs as u64))
        .build()
        .map_err(|error| error.to_string())?;
    let (system_prompt, user_prompt) = build_ai_prompts(context);

    let attempts = settings.retry_count.max(1);
    let mut last_error = "provider_call_unknown_error".to_string();
    for attempt in 1..=attempts {
        let call_result = match settings.provider {
            AiProvider::Qwen
            | AiProvider::Doubao
            | AiProvider::Deepseek
            | AiProvider::Glm
            | AiProvider::OpenApi => {
                call_openai_compatible_provider(&client, settings, &system_prompt, &user_prompt)
            }
            AiProvider::Minimax => {
                call_minimax_provider(&client, settings, &system_prompt, &user_prompt)
            }
        };

        match call_result {
            Ok(content) => {
                let parsed = parse_ai_provider_response(&content)?;
                return Ok(ensure_analysis_payload(parsed, fallback));
            }
            Err(error) => {
                last_error = error;
                if attempt < attempts {
                    std::thread::sleep(Duration::from_millis(280));
                }
            }
        }
    }

    Err(last_error)
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
fn merge_candidate_import(
    state: State<'_, AppState>,
    input: MergeCandidateImportInput,
) -> Result<Candidate, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;

    let existing = conn
        .query_row(
            "SELECT tags_json FROM candidates WHERE id = ?1",
            [input.candidate_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    let existing_tags_json = existing
        .ok_or_else(|| format!("Candidate {} not found", input.candidate_id))?;
    let existing_tags: Vec<String> =
        serde_json::from_str(&existing_tags_json).unwrap_or_default();
    let incoming_tags = input.tags.unwrap_or_default();
    let merged_tags = merge_candidate_tags(&existing_tags, &incoming_tags);
    let merged_tags_json = serde_json::to_string(&merged_tags).map_err(|error| error.to_string())?;

    let incoming_company = input
        .current_company
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let incoming_years = input.years_of_experience.map(|value| value.max(0.0));

    let normalized_phone = input
        .phone
        .as_deref()
        .map(normalize_phone)
        .filter(|value| !value.is_empty());
    let phone_hash = normalized_phone.as_deref().map(hash_value);
    let phone_enc = normalized_phone
        .as_deref()
        .map(|value| state.cipher.encrypt(value))
        .transpose()
        .map_err(|error| error.to_string())?;

    let normalized_email = input
        .email
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let email_hash = normalized_email.as_deref().map(hash_value);
    let email_enc = normalized_email
        .as_deref()
        .map(|value| state.cipher.encrypt(value))
        .transpose()
        .map_err(|error| error.to_string())?;

    let now = now_iso();
    let updated = conn
        .execute(
            r#"
            UPDATE candidates
            SET
                current_company = CASE
                    WHEN (current_company IS NULL OR trim(current_company) = '') AND ?1 IS NOT NULL
                    THEN ?1
                    ELSE current_company
                END,
                years_of_experience = CASE
                    WHEN ?2 IS NOT NULL AND ?2 > years_of_experience
                    THEN ?2
                    ELSE years_of_experience
                END,
                tags_json = ?3,
                phone_enc = COALESCE(phone_enc, ?4),
                phone_hash = COALESCE(phone_hash, ?5),
                email_enc = COALESCE(email_enc, ?6),
                email_hash = COALESCE(email_hash, ?7),
                updated_at = ?8
            WHERE id = ?9
            "#,
            params![
                incoming_company,
                incoming_years,
                merged_tags_json,
                phone_enc,
                phone_hash,
                email_enc,
                email_hash,
                now,
                input.candidate_id,
            ],
        )
        .map_err(|error| error.to_string())?;

    if updated == 0 {
        return Err(format!("Candidate {} not found", input.candidate_id));
    }

    if let Some(job_id) = input.job_id {
        let stage_text: String = conn
            .query_row(
                "SELECT stage FROM candidates WHERE id = ?1",
                [input.candidate_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;

        conn.execute(
            r#"
            INSERT INTO applications(job_id, candidate_id, stage, notes, created_at, updated_at)
            VALUES (?1, ?2, ?3, NULL, ?4, ?5)
            ON CONFLICT(job_id, candidate_id)
            DO UPDATE SET stage = excluded.stage, updated_at = excluded.updated_at
            "#,
            params![job_id, input.candidate_id, stage_text, now, now],
        )
        .map_err(|error| error.to_string())?;
    }

    sync_candidate_search(&conn, input.candidate_id).map_err(|error| error.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT id, external_id, source, name, current_company, years_of_experience, stage, tags_json, phone_enc, email_enc, created_at, updated_at FROM candidates WHERE id = ?1",
        )
        .map_err(|error| error.to_string())?;
    let candidate = stmt
        .query_row([input.candidate_id], |row| candidate_from_row(row, &state.cipher))
        .map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "candidate.merge",
        "candidate",
        Some(candidate.id.to_string()),
        serde_json::json!({
            "jobId": input.job_id,
            "mergedTagCount": candidate.tags.len(),
            "hadPhoneInput": normalized_phone.is_some(),
            "hadEmailInput": normalized_email.is_some()
        }),
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

fn round_one_decimal(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn default_screening_dimensions() -> Vec<ScreeningDimension> {
    vec![
        ScreeningDimension {
            key: "goal_orientation".to_string(),
            label: "目标导向".to_string(),
            weight: 30,
        },
        ScreeningDimension {
            key: "team_collaboration".to_string(),
            label: "团队协作".to_string(),
            weight: 15,
        },
        ScreeningDimension {
            key: "self_drive".to_string(),
            label: "自驱力".to_string(),
            weight: 15,
        },
        ScreeningDimension {
            key: "reflection_iteration".to_string(),
            label: "反思迭代".to_string(),
            weight: 10,
        },
        ScreeningDimension {
            key: "openness".to_string(),
            label: "开放性".to_string(),
            weight: 8,
        },
        ScreeningDimension {
            key: "resilience".to_string(),
            label: "抗压韧性".to_string(),
            weight: 7,
        },
        ScreeningDimension {
            key: "learning_ability".to_string(),
            label: "学习能力".to_string(),
            weight: 10,
        },
        ScreeningDimension {
            key: "values_fit".to_string(),
            label: "价值观契合".to_string(),
            weight: 5,
        },
    ]
}

fn normalize_screening_dimensions(
    dimensions: Option<Vec<ScreeningDimension>>,
) -> Result<Vec<ScreeningDimension>, String> {
    let raw = dimensions.unwrap_or_else(default_screening_dimensions);
    if raw.is_empty() {
        return Err("screening_dimensions_empty".to_string());
    }

    let mut seen = BTreeMap::<String, bool>::new();
    let mut normalized = Vec::<ScreeningDimension>::new();
    let mut total_weight = 0_i32;

    for item in raw {
        let key = item.key.trim().to_lowercase();
        let label = item.label.trim().to_string();
        if key.is_empty() || label.is_empty() {
            return Err("screening_dimension_key_or_label_empty".to_string());
        }
        if item.weight <= 0 {
            return Err("screening_dimension_weight_must_be_positive".to_string());
        }
        if seen.insert(key.clone(), true).is_some() {
            return Err(format!("screening_dimension_key_duplicate:{key}"));
        }

        total_weight += item.weight;
        normalized.push(ScreeningDimension {
            key,
            label,
            weight: item.weight,
        });
    }

    if total_weight != 100 {
        return Err(format!(
            "screening_dimension_weight_sum_invalid:{total_weight}"
        ));
    }

    Ok(normalized)
}

fn parse_dimensions_from_config(value: &Value) -> Option<Vec<ScreeningDimension>> {
    let array = value
        .get("dimensions")
        .and_then(|item| item.as_array())
        .cloned()?;
    let mut dimensions = Vec::new();
    for item in array {
        let key = item.get("key").and_then(|v| v.as_str()).unwrap_or("").trim();
        let label = item
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let weight = item.get("weight").and_then(|v| v.as_i64()).unwrap_or(0);
        if key.is_empty() || label.is_empty() || weight <= 0 {
            continue;
        }
        dimensions.push(ScreeningDimension {
            key: key.to_lowercase(),
            label: label.to_string(),
            weight: weight as i32,
        });
    }

    if dimensions.is_empty() {
        None
    } else {
        Some(dimensions)
    }
}

fn load_screening_dimensions(
    conn: &Connection,
    template_id: i64,
) -> Result<Vec<ScreeningDimension>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT dimension_key, dimension_label, weight
            FROM screening_dimensions
            WHERE template_id = ?1
            ORDER BY sort_order ASC
            "#,
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([template_id], |row| {
            Ok(ScreeningDimension {
                key: row.get(0)?,
                label: row.get(1)?,
                weight: row.get(2)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn read_screening_template_by_id(
    conn: &Connection,
    template_id: i64,
) -> Result<ScreeningTemplateRecord, String> {
    let row = conn
        .query_row(
            r#"
            SELECT id, scope, job_id, name, config_json, created_at, updated_at
            FROM screening_templates
            WHERE id = ?1
            "#,
            [template_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .map_err(|error| error.to_string())?;

    let config_json: Value = serde_json::from_str(&row.4).unwrap_or(Value::Null);
    let mut dimensions = load_screening_dimensions(conn, row.0)?;
    if dimensions.is_empty() {
        dimensions = parse_dimensions_from_config(&config_json)
            .unwrap_or_else(default_screening_dimensions);
    }
    let risk_rules = config_json
        .get("riskRules")
        .cloned()
        .or_else(|| config_json.get("risk_rules").cloned())
        .unwrap_or_else(|| serde_json::json!({}));

    Ok(ScreeningTemplateRecord {
        id: row.0,
        scope: row.1,
        job_id: row.2,
        name: row.3,
        dimensions,
        risk_rules,
        created_at: row.5,
        updated_at: row.6,
    })
}

fn upsert_screening_template_internal(
    conn: &Connection,
    scope: &str,
    job_id: Option<i64>,
    name: String,
    dimensions: Vec<ScreeningDimension>,
    risk_rules: Value,
) -> Result<ScreeningTemplateRecord, String> {
    let now = now_iso();
    let existing_id = if let Some(job_id_value) = job_id {
        conn.query_row(
            "SELECT id FROM screening_templates WHERE scope = ?1 AND job_id = ?2 LIMIT 1",
            params![scope, job_id_value],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
    } else {
        conn.query_row(
            "SELECT id FROM screening_templates WHERE scope = ?1 AND job_id IS NULL LIMIT 1",
            [scope],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
    };

    let config_json = serde_json::json!({
        "dimensions": dimensions,
        "riskRules": risk_rules,
    })
    .to_string();

    let template_id = if let Some(existing) = existing_id {
        conn.execute(
            r#"
            UPDATE screening_templates
            SET name = ?1, config_json = ?2, updated_at = ?3
            WHERE id = ?4
            "#,
            params![name, config_json, now, existing],
        )
        .map_err(|error| error.to_string())?;
        existing
    } else {
        conn.execute(
            r#"
            INSERT INTO screening_templates(scope, job_id, name, config_json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![scope, job_id, name, config_json, now, now],
        )
        .map_err(|error| error.to_string())?;
        conn.last_insert_rowid()
    };

    conn.execute(
        "DELETE FROM screening_dimensions WHERE template_id = ?1",
        [template_id],
    )
    .map_err(|error| error.to_string())?;

    let final_template = read_screening_template_by_id(conn, template_id)?;
    for (index, dimension) in final_template.dimensions.iter().enumerate() {
        conn.execute(
            r#"
            INSERT INTO screening_dimensions(
                template_id, dimension_key, dimension_label, weight, sort_order, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                template_id,
                dimension.key,
                dimension.label,
                dimension.weight,
                index as i32,
                now,
                now,
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    if scope == "job" {
        if let Some(job_id_value) = job_id {
            conn.execute(
                r#"
                INSERT INTO job_screening_overrides(job_id, template_id, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(job_id)
                DO UPDATE SET template_id = excluded.template_id, updated_at = excluded.updated_at
                "#,
                params![job_id_value, template_id, now, now],
            )
            .map_err(|error| error.to_string())?;
        }
    }

    read_screening_template_by_id(conn, template_id)
}

fn resolve_screening_template(
    conn: &Connection,
    job_id: Option<i64>,
) -> Result<ScreeningTemplateRecord, String> {
    if let Some(job_id_value) = job_id {
        let template_id = conn
            .query_row(
                r#"
                SELECT st.id
                FROM job_screening_overrides jo
                JOIN screening_templates st ON st.id = jo.template_id
                WHERE jo.job_id = ?1
                LIMIT 1
                "#,
                [job_id_value],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;

        if let Some(id) = template_id {
            return read_screening_template_by_id(conn, id);
        }
    }

    let global_template_id = conn
        .query_row(
            r#"
            SELECT id
            FROM screening_templates
            WHERE scope = 'global'
            ORDER BY updated_at DESC
            LIMIT 1
            "#,
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    if let Some(id) = global_template_id {
        return read_screening_template_by_id(conn, id);
    }

    upsert_screening_template_internal(
        conn,
        "global",
        None,
        "默认筛选模板".to_string(),
        normalize_screening_dimensions(None)?,
        serde_json::json!({}),
    )
}

fn parse_job_required_skills(description: &str) -> Vec<String> {
    description
        .split(|char: char| !char.is_alphanumeric() && char != '+')
        .filter(|token| token.len() >= 3)
        .take(10)
        .map(|token| token.to_lowercase())
        .collect()
}

fn parse_job_salary_max(salary_text: &str) -> Option<f64> {
    salary_text
        .split(|item| item == '-' || item == '~' || item == '到')
        .filter_map(|item| item.trim().parse::<f64>().ok())
        .max_by(|left, right| left.total_cmp(right))
}

fn normalize_interview_questions(
    questions: Vec<InterviewQuestion>,
) -> Result<Vec<InterviewQuestion>, String> {
    if questions.is_empty() {
        return Err("interview_questions_empty".to_string());
    }

    let mut normalized = Vec::new();
    for item in questions {
        let primary_question = item.primary_question.trim().to_string();
        if primary_question.is_empty() {
            return Err("interview_question_primary_empty".to_string());
        }

        let follow_ups = item
            .follow_ups
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        let scoring_points = item
            .scoring_points
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        let red_flags = item
            .red_flags
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();

        if follow_ups.is_empty() {
            return Err("interview_followups_empty".to_string());
        }
        if scoring_points.is_empty() {
            return Err("interview_scoring_points_empty".to_string());
        }

        normalized.push(InterviewQuestion {
            primary_question,
            follow_ups,
            scoring_points,
            red_flags,
        });
    }

    Ok(normalized)
}

fn build_interview_slot_key(candidate_id: i64, job_id: Option<i64>) -> String {
    format!("{candidate_id}:{}", job_id.unwrap_or_default())
}

fn collect_numeric_scores(value: &Value, scores: &mut Vec<f64>) {
    match value {
        Value::Number(number) => {
            if let Some(value) = number.as_f64() {
                scores.push(value);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_numeric_scores(item, scores);
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                collect_numeric_scores(item, scores);
            }
        }
        _ => {}
    }
}

fn collect_string_values(value: &Value) -> Vec<String> {
    match value {
        Value::String(text) => {
            let normalized = text.trim();
            if normalized.is_empty() {
                Vec::new()
            } else {
                vec![normalized.to_string()]
            }
        }
        Value::Array(items) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::trim))
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn build_interview_evidence(transcript: &str) -> Vec<String> {
    let mut evidence = transcript
        .lines()
        .map(str::trim)
        .filter(|line| line.chars().count() >= 8)
        .take(3)
        .map(|line| trim_resume_excerpt(line, 120))
        .collect::<Vec<_>>();

    if evidence.is_empty() {
        let fallback = trim_resume_excerpt(transcript.trim(), 120);
        if !fallback.is_empty() {
            evidence.push(fallback);
        }
    }

    evidence
}

fn build_generated_interview_questions(
    role_title: Option<&str>,
    candidate_name: &str,
    years_of_experience: f64,
    required_skills: &[String],
    extracted_skills: &[String],
    screening_recommendation: Option<&str>,
    screening_risk_level: Option<&str>,
    latest_analysis_risks: &[String],
) -> Vec<InterviewQuestion> {
    let role_label = role_title.unwrap_or("目标岗位");
    let normalized_extracted = extracted_skills
        .iter()
        .map(|item| item.to_lowercase())
        .collect::<Vec<_>>();
    let primary_skill = extracted_skills
        .first()
        .cloned()
        .or_else(|| required_skills.first().cloned())
        .unwrap_or_else(|| "岗位核心能力".to_string());
    let missing_skills = required_skills
        .iter()
        .filter(|item| !normalized_extracted.iter().any(|owned| owned.contains(&item.to_lowercase())))
        .take(2)
        .cloned()
        .collect::<Vec<_>>();

    let mut questions = vec![
        InterviewQuestion {
            primary_question: format!(
                "请你复盘最近一个最有代表性的项目，重点说明你如何用 {} 在 {} 中达成可量化结果。",
                primary_skill, role_label
            ),
            follow_ups: vec![
                "这个项目你负责的关键决策点是什么？".to_string(),
                "当方案受限时你如何权衡进度、质量和成本？".to_string(),
                "如果重做一次你会优先优化哪一部分？".to_string(),
            ],
            scoring_points: vec![
                "能讲清目标、约束、动作与结果链路".to_string(),
                "有可验证指标（效率、收益、稳定性等）".to_string(),
                "具备复盘与迭代意识".to_string(),
            ],
            red_flags: vec![
                "项目描述停留在职责罗列，无具体结果".to_string(),
                "无法说明个人贡献与团队贡献边界".to_string(),
            ],
        },
        InterviewQuestion {
            primary_question: format!(
                "请描述一次跨团队协作推动复杂事项落地的经历，{} 在其中承担了什么角色？",
                candidate_name
            ),
            follow_ups: vec![
                "冲突或分歧是如何被解决的？".to_string(),
                "你如何管理不同角色的预期？".to_string(),
                "出现延期时你如何对齐优先级？".to_string(),
            ],
            scoring_points: vec![
                "能体现协作、沟通与影响力".to_string(),
                "对风险管理和推进节奏有方法".to_string(),
                "复盘中能体现团队视角".to_string(),
            ],
            red_flags: vec![
                "把问题完全归因于他人".to_string(),
                "回避沟通与责任承担".to_string(),
            ],
        },
    ];

    if !missing_skills.is_empty() {
        questions.push(InterviewQuestion {
            primary_question: format!(
                "JD 中包含 {}，但你简历证据较少。请给出可迁移经验并说明 30 天补齐计划。",
                missing_skills.join(" / ")
            ),
            follow_ups: vec![
                "哪些能力可以迁移，哪些需要补课？".to_string(),
                "你会如何验证补齐后的产出质量？".to_string(),
            ],
            scoring_points: vec![
                "迁移路径清晰且有落地动作".to_string(),
                "能给出可执行学习计划和验证标准".to_string(),
            ],
            red_flags: vec![
                "无法说明补齐方案，仅泛泛而谈".to_string(),
                "对关键能力差距没有风险意识".to_string(),
            ],
        });
    } else {
        questions.push(InterviewQuestion {
            primary_question: format!("请现场拆解一个 {} 场景中的复杂问题，你会如何定义成功标准？", role_label),
            follow_ups: vec![
                "第一周和第一个月的推进计划是什么？".to_string(),
                "你会如何设计监控指标和回滚策略？".to_string(),
            ],
            scoring_points: vec![
                "问题拆解完整，优先级清晰".to_string(),
                "有工程落地和风险兜底意识".to_string(),
            ],
            red_flags: vec![
                "只讲理念，不给执行路径".to_string(),
                "忽略风险与兜底机制".to_string(),
            ],
        });
    }

    let risk_topic = if screening_risk_level == Some("HIGH")
        || screening_recommendation == Some("REVIEW")
        || !latest_analysis_risks.is_empty()
    {
        "请针对你过去经历中最可能影响岗位胜任力的风险点做一次主动说明。"
    } else {
        "请举例说明你在高压交付场景下如何维持质量与协作。"
    };
    questions.push(InterviewQuestion {
        primary_question: risk_topic.to_string(),
        follow_ups: vec![
            "你如何识别早期预警信号？".to_string(),
            "若再次发生类似情况你会如何调整？".to_string(),
        ],
        scoring_points: vec![
            "能正视风险，不回避问题".to_string(),
            "提出可执行的预防和修复策略".to_string(),
        ],
        red_flags: vec![
            "把风险解释为“运气不好”且无改进方案".to_string(),
            "对失败复盘缺失".to_string(),
        ],
    });

    questions.push(InterviewQuestion {
        primary_question: format!(
            "结合你 {} 年经验，为什么你认为自己当前阶段适合这个岗位？",
            years_of_experience
        ),
        follow_ups: vec![
            "入职前 90 天你最希望达成的目标是什么？".to_string(),
            "如果实际岗位与预期不一致，你会如何调整？".to_string(),
        ],
        scoring_points: vec![
            "动机真实且与岗位目标一致".to_string(),
            "对业务和个人发展路径有清晰预期".to_string(),
        ],
        red_flags: vec![
            "求职动机仅围绕薪资且回避岗位挑战".to_string(),
            "对岗位内容和业务缺乏理解".to_string(),
        ],
    });

    questions
}

fn evaluate_interview_feedback_payload(
    transcript_text: &str,
    structured_feedback: &Value,
) -> InterviewEvaluationPayload {
    let transcript_len = transcript_text.chars().count();
    let mut raw_scores = Vec::new();
    if let Some(scores) = structured_feedback.get("scores") {
        collect_numeric_scores(scores, &mut raw_scores);
    } else {
        collect_numeric_scores(structured_feedback, &mut raw_scores);
    }

    let normalized_scores = raw_scores
        .into_iter()
        .filter(|value| value.is_finite())
        .map(|value| if value > 0.0 && value <= 1.0 { value * 5.0 } else { value })
        .map(|value| value.clamp(0.0, 5.0))
        .collect::<Vec<_>>();
    let score_count = normalized_scores.len();
    let score_avg = if score_count == 0 {
        0.0
    } else {
        normalized_scores.iter().sum::<f64>() / score_count as f64
    };

    let transcript_quality = if transcript_len >= 900 {
        92.0
    } else if transcript_len >= 600 {
        84.0
    } else if transcript_len >= 320 {
        74.0
    } else if transcript_len >= 120 {
        64.0
    } else {
        46.0
    };
    let structured_quality = if score_count == 0 {
        48.0
    } else {
        score_avg * 20.0
    };
    let mut overall_score = clamp_score((structured_quality * 0.7 + transcript_quality * 0.3).round() as i32);

    let mut evidence = build_interview_evidence(transcript_text);
    if let Some(summary) = structured_feedback
        .get("summary")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        evidence.push(format!(
            "面试官总结: {}",
            trim_resume_excerpt(summary, 120)
        ));
    }
    let red_flags = structured_feedback
        .get("red_flags")
        .map(collect_string_values)
        .unwrap_or_default();
    if !red_flags.is_empty() {
        evidence.push(format!("红旗信号: {}", red_flags.join("；")));
    }

    let mut verification_points = Vec::<String>::new();
    if transcript_len < 120 {
        verification_points.push("面试转写文本不足，请补充完整问答记录。".to_string());
    }
    if score_count < 3 {
        verification_points.push("结构化评分维度不足，至少补充 3 个维度评分。".to_string());
    }
    if evidence.len() < 2 {
        verification_points.push("可引用证据不足，请补充关键问答片段。".to_string());
    }
    if !red_flags.is_empty() {
        verification_points.push("存在红旗信号，建议安排补充追问并交叉验证。".to_string());
    }

    let evidence_insufficient = transcript_len < 120 || score_count < 3 || evidence.len() < 2;
    if evidence_insufficient {
        overall_score = overall_score.min(65);
        if verification_points.is_empty() {
            verification_points.push("当前证据不足，建议补充二面后再决策。".to_string());
        }
        return InterviewEvaluationPayload {
            recommendation: "HOLD".to_string(),
            overall_score,
            confidence: 0.42,
            evidence,
            verification_points,
            uncertainty: "证据不足，当前结论稳定性较低。".to_string(),
        };
    }

    let recommendation = if overall_score >= 80 && score_avg >= 4.0 && red_flags.is_empty() {
        "HIRE"
    } else if overall_score >= 60 && score_avg >= 3.0 {
        "HOLD"
    } else {
        "NO_HIRE"
    }
    .to_string();

    if verification_points.is_empty() {
        if recommendation == "HIRE" {
            verification_points.push("建议安排业务复核面，确认关键场景匹配度。".to_string());
        } else if recommendation == "HOLD" {
            verification_points.push("建议进行补充面，聚焦风险点做定向验证。".to_string());
        } else {
            verification_points.push("如需复议，需补充与风险点相反的客观证据。".to_string());
        }
    }

    let confidence = (0.52
        + (score_count.min(8) as f64) * 0.04
        + (transcript_len.min(1200) as f64 / 1200.0) * 0.18)
        .clamp(0.45, 0.93);
    let uncertainty = if recommendation == "HIRE" {
        "结论较稳定，但仍需关注业务场景迁移风险。"
    } else if recommendation == "HOLD" {
        "存在可提升空间，建议补充关键证据后复评。"
    } else {
        "当前证据显示匹配度不足，结论偏稳定。"
    }
    .to_string();

    InterviewEvaluationPayload {
        recommendation,
        overall_score,
        confidence,
        evidence,
        verification_points,
        uncertainty,
    }
}

fn dimension_signal_score(key: &str, resume_lower: &str, years: f64) -> f64 {
    let keywords: &[&str] = match key {
        "goal_orientation" => &["目标", "结果", "交付", "增长", "kpi", "指标", "owner"],
        "team_collaboration" => &["协作", "团队", "跨部门", "沟通", "配合"],
        "self_drive" => &["主动", "自驱", "独立", "推进", "负责到底"],
        "reflection_iteration" => &["复盘", "迭代", "优化", "改进", "总结"],
        "openness" => &["开放", "反馈", "接受建议", "新技术", "尝试"],
        "resilience" => &["压力", "抗压", "紧急", "高并发", "故障恢复"],
        "learning_ability" => &["学习", "研究", "调研", "证书", "培训"],
        "values_fit" => &["价值观", "诚信", "责任心", "客户", "长期主义"],
        _ => &["项目", "负责", "协作", "优化"],
    };

    let mut score = 3.0_f64;
    let keyword_hits = keywords
        .iter()
        .filter(|keyword| resume_lower.contains(**keyword))
        .count() as f64;
    score += (keyword_hits.min(4.0)) * 0.3;

    if years >= 5.0 {
        score += 0.3;
    } else if years < 1.5 {
        score -= 0.3;
    }

    if resume_lower.len() < 120 {
        score -= 0.3;
    }

    score.clamp(1.0, 5.0)
}

fn normalize_resume_text(text: &str) -> String {
    text.replace('\u{00a0}', " ")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn decode_xml_entities(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn extract_docx_xml_text(xml_bytes: &[u8]) -> Result<String, String> {
    let xml_text = String::from_utf8(xml_bytes.to_vec()).map_err(|error| error.to_string())?;
    let regex = Regex::new(r"(?s)<w:t[^>]*>(.*?)</w:t>").map_err(|error| error.to_string())?;
    let mut parts = Vec::new();
    for capture in regex.captures_iter(&xml_text) {
        if let Some(content) = capture.get(1) {
            let text = decode_xml_entities(content.as_str()).trim().to_string();
            if !text.is_empty() {
                parts.push(text);
            }
        }
    }

    Ok(parts.join(" "))
}

fn extract_text_from_docx_bytes(bytes: &[u8]) -> Result<String, String> {
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|error| error.to_string())?;

    let mut sections = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|error| error.to_string())?;
        let name = file.name().to_string();
        if name == "word/document.xml"
            || name.starts_with("word/header")
            || name.starts_with("word/footer")
        {
            let mut xml = Vec::new();
            file.read_to_end(&mut xml).map_err(|error| error.to_string())?;
            let text = extract_docx_xml_text(&xml)?;
            if !text.trim().is_empty() {
                sections.push(text);
            }
        }
    }

    Ok(normalize_resume_text(&sections.join("\n")))
}

fn extract_text_from_pdf_bytes(bytes: &[u8]) -> Result<String, String> {
    let text = pdf_extract::extract_text_from_mem(bytes).map_err(|error| error.to_string())?;
    Ok(normalize_resume_text(&text))
}

fn extract_text_from_plain_bytes(bytes: &[u8]) -> Result<String, String> {
    let text = String::from_utf8(bytes.to_vec()).map_err(|error| error.to_string())?;
    Ok(normalize_resume_text(&text))
}

fn extract_file_extension(file_name: &str) -> String {
    Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .trim()
        .to_lowercase()
}

fn try_tesseract_ocr(bytes: &[u8], extension: &str) -> Result<String, String> {
    let probe = Command::new("tesseract")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if !matches!(probe, Ok(status) if status.success()) {
        return Err("tesseract_not_available".to_string());
    }

    let token = format!(
        "doss-ocr-{}-{}",
        Utc::now().timestamp_millis(),
        rand::random::<u32>()
    );
    let tmp_dir = std::env::temp_dir();
    let input_path = tmp_dir.join(format!("{token}.{extension}"));
    let output_base = tmp_dir.join(format!("{token}-out"));
    let output_text_path = output_base.with_extension("txt");

    fs::write(&input_path, bytes).map_err(|error| error.to_string())?;

    let status = Command::new("tesseract")
        .arg(&input_path)
        .arg(&output_base)
        .arg("-l")
        .arg("chi_sim+eng")
        .status()
        .map_err(|error| error.to_string())?;

    if !status.success() {
        let fallback_status = Command::new("tesseract")
            .arg(&input_path)
            .arg(&output_base)
            .arg("-l")
            .arg("eng")
            .status()
            .map_err(|error| error.to_string())?;
        if !fallback_status.success() {
            let _ = fs::remove_file(&input_path);
            let _ = fs::remove_file(&output_text_path);
            return Err("tesseract_ocr_failed".to_string());
        }
    }

    let text = fs::read_to_string(&output_text_path).map_err(|error| error.to_string())?;
    let _ = fs::remove_file(&input_path);
    let _ = fs::remove_file(&output_text_path);

    Ok(normalize_resume_text(&text))
}

fn build_structured_resume_fields(raw_text: &str) -> Value {
    let lowered = raw_text.to_lowercase();

    let skill_catalog: &[(&str, &[&str])] = &[
        ("Vue3", &["vue3", "vue.js", "vue"]),
        ("TypeScript", &["typescript", "ts"]),
        ("JavaScript", &["javascript", "js"]),
        ("React", &["react"]),
        ("Node.js", &["node.js", "nodejs", "node"]),
        ("Playwright", &["playwright"]),
        ("SQL", &["sql", "mysql", "postgres", "sqlite"]),
        ("Rust", &["rust"]),
        ("Python", &["python"]),
        ("Java", &["java"]),
        ("Go", &["golang", "go"]),
    ];

    let mut skills = Vec::<String>::new();
    for (label, keywords) in skill_catalog {
        if keywords.iter().any(|keyword| lowered.contains(keyword)) {
            skills.push(label.to_string());
        }
    }

    let years_regex = Regex::new(r"(?i)(\d{1,2}(?:\.\d+)?)\s*年").expect("years regex");
    let years_of_experience = years_regex
        .captures_iter(raw_text)
        .filter_map(|capture| capture.get(1).and_then(|value| value.as_str().parse::<f64>().ok()))
        .fold(0.0_f64, f64::max);

    let salary_context_regex = Regex::new(
        r"(?i)(?:期望薪资|期望|薪资|薪酬|salary)[^\d]{0,8}(\d{1,3})(?:\s*[-~到]\s*(\d{1,3}))?\s*[kK千]",
    )
    .expect("salary context regex");
    let generic_salary_regex = Regex::new(r"(?i)\b(\d{1,3})\s*[kK千]\b").expect("salary regex");

    let expected_salary_k = salary_context_regex
        .captures(raw_text)
        .and_then(|capture| {
            capture
                .get(2)
                .or_else(|| capture.get(1))
                .and_then(|value| value.as_str().parse::<f64>().ok())
        })
        .or_else(|| {
            generic_salary_regex
                .captures(raw_text)
                .and_then(|capture| capture.get(1))
                .and_then(|value| value.as_str().parse::<f64>().ok())
        });

    let education_level = if raw_text.contains("博士") {
        Some("博士")
    } else if raw_text.contains("硕士") {
        Some("硕士")
    } else if raw_text.contains("本科") {
        Some("本科")
    } else if raw_text.contains("大专") {
        Some("大专")
    } else {
        None
    };

    let school_regex = Regex::new(r"([^\s]{2,16}(大学|学院))").expect("school regex");
    let schools = school_regex
        .captures_iter(raw_text)
        .filter_map(|capture| capture.get(1).map(|value| value.as_str().to_string()))
        .take(3)
        .collect::<Vec<_>>();

    let stability_hints = if years_of_experience > 0.0 {
        if years_of_experience < 2.0 {
            vec!["工作年限较短，建议重点验证稳定性".to_string()]
        } else if years_of_experience >= 5.0 {
            vec!["工作年限较长，可优先评估深度与带人经验".to_string()]
        } else {
            vec!["工作年限中等，建议结合项目复杂度综合判断".to_string()]
        }
    } else {
        Vec::new()
    };

    let project_mentions = raw_text.matches("项目").count() as i64;
    let summary = raw_text.chars().take(220).collect::<String>();

    serde_json::json!({
        "skills": skills,
        "yearsOfExperience": if years_of_experience > 0.0 { Some(years_of_experience) } else { None::<f64> },
        "expectedSalaryK": expected_salary_k,
        "education": {
            "level": education_level,
            "schools": schools,
        },
        "projectMentions": project_mentions,
        "stabilityHints": stability_hints,
        "summary": summary,
    })
}

#[tauri::command]
fn parse_resume_file(input: ParseResumeFileInput) -> Result<ParseResumeFileOutput, String> {
    let bytes = BASE64_STANDARD
        .decode(input.content_base64.trim())
        .map_err(|error| error.to_string())?;

    let extension = extract_file_extension(&input.file_name);
    let enable_ocr = input.enable_ocr.unwrap_or(false);

    let mut raw_text = match extension.as_str() {
        "pdf" => extract_text_from_pdf_bytes(&bytes),
        "docx" => extract_text_from_docx_bytes(&bytes),
        "txt" | "md" => extract_text_from_plain_bytes(&bytes),
        "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff" => Ok(String::new()),
        _ => Err(format!("unsupported_resume_file_type: {}", extension)),
    }?;

    let mut ocr_used = false;
    if enable_ocr
        && raw_text.trim().is_empty()
        && matches!(
            extension.as_str(),
            "pdf" | "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff"
        )
    {
        if let Ok(ocr_text) = try_tesseract_ocr(&bytes, &extension) {
            if !ocr_text.trim().is_empty() {
                raw_text = ocr_text;
                ocr_used = true;
            }
        }
    }

    if raw_text.trim().is_empty() {
        return Err("resume_text_empty_after_parse".to_string());
    }

    let normalized = normalize_resume_text(&raw_text);
    let parsed = build_structured_resume_fields(&normalized);

    Ok(ParseResumeFileOutput {
        raw_text: normalized,
        parsed,
        metadata: serde_json::json!({
            "fileName": input.file_name,
            "extension": extension,
            "size": bytes.len(),
            "ocrUsed": ocr_used,
        }),
    })
}

#[tauri::command]
fn get_ai_provider_catalog() -> Result<AiProviderCatalogView, String> {
    let providers = AiProvider::all()
        .iter()
        .map(AiProvider::to_catalog_item)
        .collect::<Vec<_>>();
    Ok(AiProviderCatalogView {
        providers,
        updated_at: now_iso(),
    })
}

#[tauri::command]
fn list_ai_provider_profiles(state: State<'_, AppState>) -> Result<Vec<AiProviderProfileView>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let profiles = read_ai_profiles(&conn).map_err(|error| error.to_string())?;
    Ok(to_ai_profile_views(&profiles))
}

#[tauri::command]
fn upsert_ai_provider_profile(
    state: State<'_, AppState>,
    input: UpsertAiProviderProfileInput,
) -> Result<AiProviderProfileView, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut profiles_state = read_ai_profiles(&conn).map_err(|error| error.to_string())?;
    let requested_provider = AiProvider::from_db(&input.provider);
    let target_id = input.profile_id.clone().unwrap_or_else(make_ai_profile_id);
    let now = now_iso();

    let existing_index = profiles_state
        .profiles
        .iter()
        .position(|item| item.id == target_id);

    let mut profile = if let Some(index) = existing_index {
        profiles_state.profiles[index].clone()
    } else {
        let defaults = StoredAiProviderSettings::defaults(requested_provider);
        StoredAiProviderProfile {
            id: target_id.clone(),
            name: profile_default_name(requested_provider, profiles_state.profiles.len() + 1),
            provider: requested_provider.as_db().to_string(),
            model: defaults.model,
            base_url: defaults.base_url,
            api_key_enc: None,
            temperature: defaults.temperature,
            max_tokens: defaults.max_tokens,
            timeout_secs: defaults.timeout_secs,
            retry_count: defaults.retry_count,
            created_at: now.clone(),
            updated_at: now.clone(),
        }
    };

    let previous_provider = AiProvider::from_db(&profile.provider);
    let provider_changed = previous_provider != requested_provider;

    profile.provider = requested_provider.as_db().to_string();

    if let Some(name) = input.name.as_deref().map(str::trim).filter(|item| !item.is_empty()) {
        profile.name = name.to_string();
    } else if profile.name.trim().is_empty() {
        let ordinal = existing_index.unwrap_or(profiles_state.profiles.len()) + 1;
        profile.name = profile_default_name(requested_provider, ordinal);
    }

    profile.model = input
        .model
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| item.to_string())
        .unwrap_or_else(|| {
            if provider_changed || profile.model.trim().is_empty() {
                requested_provider.default_model().to_string()
            } else {
                profile.model.trim().to_string()
            }
        });

    profile.base_url = input
        .base_url
        .as_deref()
        .map(|item| item.trim().trim_end_matches('/'))
        .filter(|item| !item.is_empty())
        .map(|item| item.to_string())
        .unwrap_or_else(|| {
            if provider_changed || profile.base_url.trim().is_empty() {
                requested_provider.default_base_url().to_string()
            } else {
                profile.base_url.trim().trim_end_matches('/').to_string()
            }
        });

    if let Some(api_key_raw) = input.api_key {
        let trimmed = api_key_raw.trim();
        profile.api_key_enc = if trimmed.is_empty() {
            None
        } else {
            Some(
                state
                    .cipher
                    .encrypt(trimmed)
                    .map_err(|error| error.to_string())?,
            )
        };
    }

    profile.temperature = input
        .temperature
        .unwrap_or(profile.temperature)
        .clamp(0.0, 1.2);
    profile.max_tokens = input.max_tokens.unwrap_or(profile.max_tokens).clamp(200, 8192);
    profile.timeout_secs = input
        .timeout_secs
        .unwrap_or(profile.timeout_secs)
        .clamp(8, 180);
    profile.retry_count = input.retry_count.unwrap_or(profile.retry_count).clamp(1, 5);
    profile.updated_at = now;
    normalize_profile_in_place(
        &mut profile,
        existing_index.unwrap_or(profiles_state.profiles.len()) + 1,
    );

    if let Some(index) = existing_index {
        profiles_state.profiles[index] = profile.clone();
    } else {
        profiles_state.profiles.push(profile.clone());
    }
    profiles_state.active_profile_id = profile.id.clone();

    write_ai_profiles(&conn, &profiles_state).map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "ai.profile.upsert",
        "settings",
        Some(profile.id.clone()),
        serde_json::json!({
            "name": profile.name,
            "provider": profile.provider,
            "model": profile.model,
            "baseUrl": profile.base_url,
            "activeProfileId": profiles_state.active_profile_id,
        }),
    )
    .map_err(|error| error.to_string())?;

    to_ai_profile_views(&profiles_state)
        .into_iter()
        .find(|item| item.id == profile.id)
        .ok_or_else(|| "ai_profile_view_not_found".to_string())
}

#[tauri::command]
fn delete_ai_provider_profile(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<Vec<AiProviderProfileView>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut profiles_state = read_ai_profiles(&conn).map_err(|error| error.to_string())?;

    if profiles_state.profiles.len() <= 1 {
        return Err("at_least_one_ai_profile_required".to_string());
    }

    let index = profiles_state
        .profiles
        .iter()
        .position(|item| item.id == profile_id)
        .ok_or_else(|| "ai_profile_not_found".to_string())?;
    let removed = profiles_state.profiles.remove(index);

    if profiles_state.active_profile_id == profile_id {
        profiles_state.active_profile_id = profiles_state
            .profiles
            .first()
            .map(|item| item.id.clone())
            .unwrap_or_default();
    }

    write_ai_profiles(&conn, &profiles_state).map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "ai.profile.delete",
        "settings",
        Some(profile_id),
        serde_json::json!({
            "removedName": removed.name,
            "activeProfileId": profiles_state.active_profile_id,
            "remaining": profiles_state.profiles.len(),
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(to_ai_profile_views(&profiles_state))
}

#[tauri::command]
fn set_default_ai_provider_profile(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<Vec<AiProviderProfileView>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut profiles_state = read_ai_profiles(&conn).map_err(|error| error.to_string())?;

    let selected = profiles_state
        .profiles
        .iter()
        .find(|item| item.id == profile_id)
        .ok_or_else(|| "ai_profile_not_found".to_string())?;

    profiles_state.active_profile_id = profile_id.clone();
    write_ai_profiles(&conn, &profiles_state).map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "ai.profile.set_default",
        "settings",
        Some(profile_id),
        serde_json::json!({
            "activeProfileId": profiles_state.active_profile_id,
            "activeProfileName": selected.name,
            "activeProvider": selected.provider,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(to_ai_profile_views(&profiles_state))
}

#[tauri::command]
async fn test_ai_provider_profile(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<AiProviderTestResult, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let settings = resolve_ai_settings_for_profile(&conn, &state.cipher, &profile_id)
        .map_err(|error| error.to_string())?;

    if settings.api_key.is_none() {
        return Err(format!("{}_api_key_missing", settings.provider.as_db()));
    }

    let endpoint = match settings.provider {
        AiProvider::Minimax => ensure_minimax_endpoint(&settings.base_url),
        _ => ensure_openai_endpoint(&settings.base_url),
    };

    let mut probe_settings = settings.clone();
    probe_settings.max_tokens = probe_settings.max_tokens.clamp(16, 256);
    let probe_settings_for_network = probe_settings.clone();
    let (response, latency_ms) = tauri::async_runtime::spawn_blocking(move || {
        probe_provider_connectivity(probe_settings_for_network)
    })
    .await
    .map_err(|error| error.to_string())??;

    match response {
        Ok(content) => {
            let reply_excerpt = trim_resume_excerpt(content.trim(), 120);
            let tested_at = now_iso();
            let _ = write_audit(
                &conn,
                "ai.profile.test",
                "settings",
                Some(profile_id),
                serde_json::json!({
                    "provider": probe_settings.provider.as_db(),
                    "model": probe_settings.model,
                    "endpoint": endpoint,
                    "latencyMs": latency_ms,
                    "ok": true,
                }),
            );
            Ok(AiProviderTestResult {
                ok: true,
                provider: probe_settings.provider.as_db().to_string(),
                model: probe_settings.model,
                endpoint,
                latency_ms,
                reply_excerpt,
                tested_at,
            })
        }
        Err(error) => {
            let _ = write_audit(
                &conn,
                "ai.profile.test",
                "settings",
                Some(profile_id),
                serde_json::json!({
                    "provider": probe_settings.provider.as_db(),
                    "model": probe_settings.model,
                    "endpoint": endpoint,
                    "latencyMs": latency_ms,
                    "ok": false,
                    "error": error,
                }),
            );
            Err(error)
        }
    }
}

#[tauri::command]
fn get_ai_provider_settings(state: State<'_, AppState>) -> Result<AiProviderSettingsView, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let settings = resolve_ai_settings(&conn, &state.cipher).map_err(|error| error.to_string())?;
    Ok(to_ai_settings_view(&settings))
}

#[tauri::command]
fn upsert_ai_provider_settings(
    state: State<'_, AppState>,
    input: UpsertAiProviderSettingsInput,
) -> Result<AiProviderSettingsView, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let provider = AiProvider::from_db(&input.provider);
    let mut stored = read_ai_settings(&conn).map_err(|error| error.to_string())?;
    let previous_provider = AiProvider::from_db(&stored.provider);
    let previous_provider_raw = stored.provider.trim().to_lowercase();
    let previous_is_legacy_mock = previous_provider_raw == "mock";
    let defaults = StoredAiProviderSettings::defaults(provider.clone());
    let api_key_changed = input.api_key.is_some();

    stored.provider = provider.as_db().to_string();
    stored.model = input
        .model
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .unwrap_or_else(|| {
            if previous_provider == provider && !previous_is_legacy_mock {
                let text = stored.model.trim();
                if text.is_empty() {
                    defaults.model.clone()
                } else {
                    text.to_string()
                }
            } else {
                defaults.model.clone()
            }
        });
    stored.base_url = input
        .base_url
        .map(|item| item.trim().trim_end_matches('/').to_string())
        .filter(|item| !item.is_empty())
        .unwrap_or_else(|| {
            if previous_provider == provider && !previous_is_legacy_mock {
                let text = stored.base_url.trim().trim_end_matches('/');
                if text.is_empty() {
                    defaults.base_url.clone()
                } else {
                    text.to_string()
                }
            } else {
                defaults.base_url.clone()
            }
        });
    stored.temperature = input
        .temperature
        .unwrap_or(stored.temperature)
        .clamp(0.0, 1.2);
    stored.max_tokens = input.max_tokens.unwrap_or(stored.max_tokens).clamp(200, 8192);
    stored.timeout_secs = input.timeout_secs.unwrap_or(stored.timeout_secs).clamp(8, 180);
    stored.retry_count = input.retry_count.unwrap_or(stored.retry_count).clamp(1, 5);

    if let Some(api_key_raw) = input.api_key {
        let trimmed = api_key_raw.trim();
        if trimmed.is_empty() {
            stored.api_key_enc = None;
        } else {
            stored.api_key_enc = Some(
                state
                    .cipher
                    .encrypt(trimmed)
                    .map_err(|error| error.to_string())?,
            );
        }
    }

    write_ai_settings(&conn, &stored).map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "ai.settings.update",
        "settings",
        Some(AI_SETTINGS_KEY.to_string()),
        serde_json::json!({
            "provider": stored.provider,
            "model": stored.model,
            "baseUrl": stored.base_url,
            "temperature": stored.temperature,
            "maxTokens": stored.max_tokens,
            "timeoutSecs": stored.timeout_secs,
            "retryCount": stored.retry_count,
            "apiKeyChanged": api_key_changed,
        }),
    )
    .map_err(|error| error.to_string())?;

    let resolved = resolve_ai_settings(&conn, &state.cipher).map_err(|error| error.to_string())?;
    Ok(to_ai_settings_view(&resolved))
}

#[tauri::command]
async fn test_ai_provider_settings(
    state: State<'_, AppState>,
    input: UpsertAiProviderSettingsInput,
) -> Result<AiProviderTestResult, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let settings = resolve_ai_settings_with_input_overrides(&conn, &state.cipher, &input)
        .map_err(|error| error.to_string())?;

    if settings.api_key.is_none() {
        return Err(format!("{}_api_key_missing", settings.provider.as_db()));
    }

    let endpoint = match settings.provider {
        AiProvider::Minimax => ensure_minimax_endpoint(&settings.base_url),
        _ => ensure_openai_endpoint(&settings.base_url),
    };

    let mut probe_settings = settings.clone();
    probe_settings.max_tokens = probe_settings.max_tokens.clamp(16, 256);
    let probe_settings_for_network = probe_settings.clone();
    let (response, latency_ms) = tauri::async_runtime::spawn_blocking(move || {
        probe_provider_connectivity(probe_settings_for_network)
    })
    .await
    .map_err(|error| error.to_string())??;

    match response {
        Ok(content) => {
            let reply_excerpt = trim_resume_excerpt(content.trim(), 120);
            let tested_at = now_iso();
            let _ = write_audit(
                &conn,
                "ai.settings.test",
                "settings",
                Some(AI_SETTINGS_KEY.to_string()),
                serde_json::json!({
                    "provider": probe_settings.provider.as_db(),
                    "model": probe_settings.model,
                    "endpoint": endpoint,
                    "latencyMs": latency_ms,
                    "ok": true,
                }),
            );
            Ok(AiProviderTestResult {
                ok: true,
                provider: probe_settings.provider.as_db().to_string(),
                model: probe_settings.model,
                endpoint,
                latency_ms,
                reply_excerpt,
                tested_at,
            })
        }
        Err(error) => {
            let _ = write_audit(
                &conn,
                "ai.settings.test",
                "settings",
                Some(AI_SETTINGS_KEY.to_string()),
                serde_json::json!({
                    "provider": probe_settings.provider.as_db(),
                    "model": probe_settings.model,
                    "endpoint": endpoint,
                    "latencyMs": latency_ms,
                    "ok": false,
                    "error": error,
                }),
            );
            Err(error)
        }
    }
}

#[tauri::command]
fn get_task_runtime_settings(state: State<'_, AppState>) -> Result<TaskRuntimeSettings, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    read_task_runtime_settings(&conn).map_err(|error| error.to_string())
}

#[tauri::command]
fn upsert_task_runtime_settings(
    state: State<'_, AppState>,
    input: UpsertTaskRuntimeSettingsInput,
) -> Result<TaskRuntimeSettings, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut settings = read_task_runtime_settings(&conn).map_err(|error| error.to_string())?;

    settings.auto_batch_concurrency = input
        .auto_batch_concurrency
        .unwrap_or(settings.auto_batch_concurrency);
    settings.auto_retry_count = input.auto_retry_count.unwrap_or(settings.auto_retry_count);
    settings.auto_retry_backoff_ms = input
        .auto_retry_backoff_ms
        .unwrap_or(settings.auto_retry_backoff_ms);

    settings = normalize_task_runtime_settings(settings);
    write_task_runtime_settings(&conn, &settings).map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "task.settings.update",
        "settings",
        Some(TASK_RUNTIME_SETTINGS_KEY.to_string()),
        serde_json::json!({
            "autoBatchConcurrency": settings.auto_batch_concurrency,
            "autoRetryCount": settings.auto_retry_count,
            "autoRetryBackoffMs": settings.auto_retry_backoff_ms,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(settings)
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

    let local_overall_score = clamp_score(
        (dimension_scores[0].score as f64 * 0.4
            + dimension_scores[1].score as f64 * 0.25
            + dimension_scores[2].score as f64 * 0.15
            + dimension_scores[3].score as f64 * 0.2)
            .round() as i32,
    );

    let local_payload = AiAnalysisPayload {
        overall_score: local_overall_score,
        dimension_scores,
        risks,
        highlights,
        suggestions,
        evidence,
        confidence: None,
    };

    let prompt_context = AiPromptContext {
        required_skills,
        extracted_skills: skills,
        candidate_years: candidate.1,
        expected_salary_k: resume_row
            .1
            .get("expectedSalaryK")
            .and_then(|value| value.as_f64()),
        max_salary_k: max_salary,
        stage: candidate.2,
        tags: candidate.3,
        resume_raw_text: resume_row.0,
        resume_parsed: resume_row.1,
    };

    let ai_settings = resolve_ai_settings(&conn, &state.cipher).map_err(|error| error.to_string())?;
    let provider_name = ai_settings.provider.as_db().to_string();
    let model_name = ai_settings.model.clone();

    let cloud_result = invoke_cloud_provider(&ai_settings, &prompt_context, &local_payload);
    let (final_payload, model_info) = match cloud_result {
        Ok(payload) => (
            payload.clone(),
            serde_json::json!({
                "provider": provider_name,
                "model": model_name,
                "generatedAt": now_iso(),
                "mode": "cloud",
                "confidence": payload.confidence,
            }),
        ),
        Err(reason) => (
            local_payload.clone(),
            serde_json::json!({
                "provider": provider_name,
                "model": model_name,
                "generatedAt": now_iso(),
                "mode": "fallback",
                "fallbackReason": reason,
            }),
        ),
    };

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
            final_payload.overall_score,
            serde_json::to_string(&final_payload.dimension_scores).map_err(|error| error.to_string())?,
            serde_json::to_string(&final_payload.risks).map_err(|error| error.to_string())?,
            serde_json::to_string(&final_payload.highlights).map_err(|error| error.to_string())?,
            serde_json::to_string(&final_payload.suggestions).map_err(|error| error.to_string())?,
            serde_json::to_string(&final_payload.evidence).map_err(|error| error.to_string())?,
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
        overall_score: final_payload.overall_score,
        dimension_scores: final_payload.dimension_scores,
        risks: final_payload.risks,
        highlights: final_payload.highlights,
        suggestions: final_payload.suggestions,
        evidence: final_payload.evidence,
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
fn get_screening_template(
    state: State<'_, AppState>,
    job_id: Option<i64>,
) -> Result<ScreeningTemplateRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    resolve_screening_template(&conn, job_id)
}

#[tauri::command]
fn upsert_screening_template(
    state: State<'_, AppState>,
    input: UpsertScreeningTemplateInput,
) -> Result<ScreeningTemplateRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let scope = if input.job_id.is_some() { "job" } else { "global" };
    let dimensions = normalize_screening_dimensions(input.dimensions)?;
    let risk_rules = input.risk_rules.unwrap_or_else(|| serde_json::json!({}));
    let name = input
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if let Some(job_id) = input.job_id {
                format!("岗位 {job_id} 微调模板")
            } else {
                "默认筛选模板".to_string()
            }
        });

    let template = upsert_screening_template_internal(
        &conn,
        scope,
        input.job_id,
        name,
        dimensions,
        risk_rules,
    )?;

    write_audit(
        &conn,
        "screening.template.upsert",
        "screening_template",
        Some(template.id.to_string()),
        serde_json::json!({
            "scope": template.scope,
            "jobId": template.job_id,
            "name": template.name,
            "dimensions": template.dimensions,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(template)
}

#[tauri::command]
fn run_resume_screening(
    state: State<'_, AppState>,
    input: RunScreeningInput,
) -> Result<ScreeningResultRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;

    let candidate_years = conn
        .query_row(
            "SELECT years_of_experience FROM candidates WHERE id = ?1",
            [input.candidate_id],
            |row| row.get::<_, f64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Candidate {} not found", input.candidate_id))?;

    let (resume_raw_text, resume_parsed): (String, Value) = conn
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
        .ok_or_else(|| "Resume required before screening".to_string())?;

    let inferred_job_id = conn
        .query_row(
            "SELECT job_id FROM applications WHERE candidate_id = ?1 ORDER BY updated_at DESC LIMIT 1",
            [input.candidate_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let effective_job_id = input.job_id.or(inferred_job_id);

    let template = resolve_screening_template(&conn, effective_job_id)?;

    let mut required_skills: Vec<String> = Vec::new();
    let mut max_salary: Option<f64> = None;
    if let Some(job_id) = effective_job_id {
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
                required_skills = parse_job_required_skills(&description_text);
            }
            if let Some(salary_text) = salary_k {
                max_salary = parse_job_salary_max(&salary_text);
            }
        }
    }

    let extracted_skills = parse_skills(&resume_parsed);
    let normalized_skills = extracted_skills
        .iter()
        .map(|skill| skill.to_lowercase())
        .collect::<Vec<_>>();
    let matched_skill_count = required_skills
        .iter()
        .filter(|required| normalized_skills.iter().any(|owned| owned.contains(*required)))
        .count();

    let skill_coverage = if required_skills.is_empty() {
        0.7_f64
    } else {
        matched_skill_count as f64 / required_skills.len() as f64
    };

    let resume_lower = resume_raw_text.to_lowercase();
    let mut t0_score = 3.0_f64;
    if candidate_years >= 5.0 {
        t0_score += 0.8;
    } else if candidate_years >= 3.0 {
        t0_score += 0.4;
    } else if candidate_years < 1.5 {
        t0_score -= 0.8;
    }
    if !required_skills.is_empty() {
        if skill_coverage >= 0.65 {
            t0_score += 0.6;
        } else if skill_coverage < 0.35 {
            t0_score -= 0.8;
        }
    }
    if resume_raw_text.chars().count() < 120 {
        t0_score -= 0.6;
    }
    t0_score = round_one_decimal(t0_score.clamp(1.0, 5.0));

    let t1_acc = template.dimensions.iter().fold(0.0_f64, |sum, dimension| {
        let score = dimension_signal_score(&dimension.key, &resume_lower, candidate_years);
        sum + (score / 5.0) * dimension.weight as f64
    });
    let t1_score = clamp_score(t1_acc.round() as i32);

    let education_level = resume_parsed
        .get("education")
        .and_then(|value| value.get("level"))
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let education_score = match education_level {
        "博士" => 95,
        "硕士" => 90,
        "本科" => 82,
        "大专" => 68,
        _ => 72,
    };

    let years_baseline = if required_skills.is_empty() {
        3.0_f64
    } else {
        (required_skills.len() as f64 / 2.0).max(2.0)
    };
    let years_match_score = clamp_score((70.0 + (candidate_years - years_baseline) * 12.0) as i32);

    let industry_risk_score = if resume_lower.contains("转行")
        || resume_lower.contains("跨行业")
        || resume_lower.contains("跨领域")
    {
        55
    } else {
        78
    };

    let expected_salary = resume_parsed
        .get("expectedSalaryK")
        .and_then(|value| value.as_f64());
    let salary_match_score = match (expected_salary, max_salary) {
        (Some(expected), Some(max)) if expected > max + 10.0 => 48,
        (Some(expected), Some(max)) if expected > max + 5.0 => 62,
        (Some(expected), Some(max)) if expected > max => 72,
        (Some(_), Some(_)) => 84,
        _ => 75,
    };

    let fine_score = clamp_score(
        ((education_score + years_match_score + industry_risk_score + salary_match_score) as f64 / 4.0)
            .round() as i32,
    );

    let project_mentions = resume_parsed
        .get("projectMentions")
        .and_then(|value| value.as_i64())
        .unwrap_or(0);

    let mut bonus_score = 0_i32;
    if project_mentions >= 3 {
        bonus_score += 4;
    } else if project_mentions >= 1 {
        bonus_score += 2;
    }
    if !required_skills.is_empty() && matched_skill_count == required_skills.len() {
        bonus_score += 4;
    }
    if normalized_skills.iter().any(|skill| {
        skill.contains("playwright") || skill.contains("rust") || skill.contains("go")
    }) {
        bonus_score += 3;
    }
    bonus_score = bonus_score.clamp(0, 15);

    let mut risk_penalty = 0_i32;
    let mut evidence = vec![
        format!("模板: {}", template.name),
        format!(
            "技能匹配: {}/{}",
            matched_skill_count,
            required_skills.len()
        ),
        format!("工作年限: {:.1} 年", candidate_years),
    ];
    let mut verification_points = Vec::<String>::new();

    if t0_score < 3.0 {
        risk_penalty += 12;
        verification_points.push("T0 硬性条件未达标，建议人工二次核验。".to_string());
    }
    if !required_skills.is_empty() && skill_coverage < 0.35 {
        risk_penalty += 10;
        verification_points.push("核心技能覆盖偏低，需在技术面重点核验。".to_string());
    }
    if resume_raw_text.chars().count() < 120 {
        risk_penalty += 8;
        verification_points.push("简历信息较少，建议补充项目证据。".to_string());
    }
    if let (Some(expected), Some(max)) = (expected_salary, max_salary) {
        if expected > max + 8.0 {
            risk_penalty += 10;
            verification_points.push("薪资预期显著高于岗位预算，需先沟通薪资边界。".to_string());
        }
    }

    let risk_level = if risk_penalty >= 18 {
        "HIGH"
    } else if risk_penalty >= 8 {
        "MEDIUM"
    } else {
        "LOW"
    }
    .to_string();

    let overall_score = clamp_score(
        (t1_score as f64 * 0.65 + fine_score as f64 * 0.35 + bonus_score as f64
            - risk_penalty as f64 * 0.8)
            .round() as i32,
    );

    let recommendation = if t0_score < 3.0 {
        "REJECT".to_string()
    } else if overall_score >= 80 && risk_level != "HIGH" {
        "PASS".to_string()
    } else if overall_score >= 65 || risk_level != "LOW" {
        "REVIEW".to_string()
    } else {
        "REJECT".to_string()
    };

    if verification_points.is_empty() {
        verification_points.push("可进入面试验证岗位关键能力与价值观匹配度。".to_string());
    }
    evidence.push(format!(
        "综合得分: {} (T1={}, 精筛={}, 加分={}, 风险扣减={})",
        overall_score, t1_score, fine_score, bonus_score, risk_penalty
    ));

    let created_at = now_iso();
    conn.execute(
        r#"
        INSERT INTO screening_results(
            candidate_id, job_id, template_id, t0_score, t1_score, fine_score,
            bonus_score, risk_penalty, overall_score, recommendation, risk_level,
            evidence_json, verification_points_json, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        "#,
        params![
            input.candidate_id,
            effective_job_id,
            Some(template.id),
            t0_score,
            t1_score,
            fine_score,
            bonus_score,
            risk_penalty,
            overall_score,
            recommendation,
            risk_level,
            serde_json::to_string(&evidence).map_err(|error| error.to_string())?,
            serde_json::to_string(&verification_points).map_err(|error| error.to_string())?,
            created_at,
        ],
    )
    .map_err(|error| error.to_string())?;

    let id = conn.last_insert_rowid();
    let result = ScreeningResultRecord {
        id,
        candidate_id: input.candidate_id,
        job_id: effective_job_id,
        template_id: Some(template.id),
        t0_score,
        t1_score,
        fine_score,
        bonus_score,
        risk_penalty,
        overall_score,
        recommendation,
        risk_level,
        evidence,
        verification_points,
        created_at,
    };

    write_audit(
        &conn,
        "screening.run",
        "screening_result",
        Some(result.id.to_string()),
        serde_json::json!({
            "candidateId": result.candidate_id,
            "jobId": result.job_id,
            "templateId": result.template_id,
            "overallScore": result.overall_score,
            "recommendation": result.recommendation,
            "riskLevel": result.risk_level,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(result)
}

#[tauri::command]
fn list_screening_results(
    state: State<'_, AppState>,
    candidate_id: i64,
) -> Result<Vec<ScreeningResultRecord>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, candidate_id, job_id, template_id, t0_score, t1_score, fine_score,
                   bonus_score, risk_penalty, overall_score, recommendation, risk_level,
                   evidence_json, verification_points_json, created_at
            FROM screening_results
            WHERE candidate_id = ?1
            ORDER BY created_at DESC
            "#,
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([candidate_id], |row| {
            let evidence_text: String = row.get(12)?;
            let verification_text: String = row.get(13)?;
            Ok(ScreeningResultRecord {
                id: row.get(0)?,
                candidate_id: row.get(1)?,
                job_id: row.get(2)?,
                template_id: row.get(3)?,
                t0_score: row.get(4)?,
                t1_score: row.get(5)?,
                fine_score: row.get(6)?,
                bonus_score: row.get(7)?,
                risk_penalty: row.get(8)?,
                overall_score: row.get(9)?,
                recommendation: row.get(10)?,
                risk_level: row.get(11)?,
                evidence: serde_json::from_str(&evidence_text).unwrap_or_default(),
                verification_points: serde_json::from_str(&verification_text).unwrap_or_default(),
                created_at: row.get(14)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn generate_interview_kit(
    state: State<'_, AppState>,
    input: GenerateInterviewKitInput,
) -> Result<InterviewKitRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;

    let (candidate_name, years_of_experience) = conn
        .query_row(
            "SELECT name, years_of_experience FROM candidates WHERE id = ?1",
            [input.candidate_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Candidate {} not found", input.candidate_id))?;

    let inferred_job_id = conn
        .query_row(
            "SELECT job_id FROM applications WHERE candidate_id = ?1 ORDER BY updated_at DESC LIMIT 1",
            [input.candidate_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let effective_job_id = input.job_id.or(inferred_job_id);

    let mut role_title: Option<String> = None;
    let mut required_skills = Vec::<String>::new();
    if let Some(job_id) = effective_job_id {
        if let Some((title, description)) = conn
            .query_row(
                "SELECT title, description FROM jobs WHERE id = ?1",
                [job_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| error.to_string())?
        {
            role_title = Some(title);
            if let Some(description_text) = description {
                required_skills = parse_job_required_skills(&description_text);
            }
        }
    }

    let resume_parsed = conn
        .query_row(
            "SELECT parsed_json FROM resumes WHERE candidate_id = ?1",
            [input.candidate_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or(Value::Null);
    let extracted_skills = parse_skills(&resume_parsed);

    let screening_hint = conn
        .query_row(
            "SELECT recommendation, risk_level FROM screening_results WHERE candidate_id = ?1 ORDER BY created_at DESC LIMIT 1",
            [input.candidate_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let latest_analysis_risks = conn
        .query_row(
            "SELECT risks_json FROM analysis_results WHERE candidate_id = ?1 ORDER BY created_at DESC LIMIT 1",
            [input.candidate_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .and_then(|text| serde_json::from_str::<Vec<String>>(&text).ok())
        .unwrap_or_default();

    let questions = build_generated_interview_questions(
        role_title.as_deref(),
        &candidate_name,
        years_of_experience,
        &required_skills,
        &extracted_skills,
        screening_hint.as_ref().map(|value| value.0.as_str()),
        screening_hint.as_ref().map(|value| value.1.as_str()),
        &latest_analysis_risks,
    );
    let now = now_iso();
    write_audit(
        &conn,
        "interview.kit.generate",
        "interview_kit",
        Some(input.candidate_id.to_string()),
        serde_json::json!({
            "candidateId": input.candidate_id,
            "jobId": effective_job_id,
            "questionCount": questions.len(),
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(InterviewKitRecord {
        id: None,
        candidate_id: input.candidate_id,
        job_id: effective_job_id,
        questions,
        generated_by: "rule-engine-v1".to_string(),
        created_at: now.clone(),
        updated_at: now,
    })
}

#[tauri::command]
fn save_interview_kit(
    state: State<'_, AppState>,
    input: SaveInterviewKitInput,
) -> Result<InterviewKitRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    conn.query_row(
        "SELECT id FROM candidates WHERE id = ?1",
        [input.candidate_id],
        |row| row.get::<_, i64>(0),
    )
    .optional()
    .map_err(|error| error.to_string())?
    .ok_or_else(|| format!("Candidate {} not found", input.candidate_id))?;

    let normalized_questions = normalize_interview_questions(input.questions)?;
    let slot_key = build_interview_slot_key(input.candidate_id, input.job_id);
    let generated_by = "rule-engine-v1".to_string();
    let now = now_iso();
    let content_json = serde_json::json!({
        "questions": normalized_questions
    })
    .to_string();

    conn.execute(
        r#"
        INSERT INTO interview_kits(slot_key, candidate_id, job_id, content_json, generated_by, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(slot_key)
        DO UPDATE SET
            candidate_id = excluded.candidate_id,
            job_id = excluded.job_id,
            content_json = excluded.content_json,
            generated_by = excluded.generated_by,
            updated_at = excluded.updated_at
        "#,
        params![
            slot_key,
            input.candidate_id,
            input.job_id,
            content_json,
            generated_by,
            now,
            now,
        ],
    )
    .map_err(|error| error.to_string())?;

    let (id, saved_generated_by, created_at, updated_at) = conn
        .query_row(
            "SELECT id, generated_by, created_at, updated_at FROM interview_kits WHERE slot_key = ?1",
            [build_interview_slot_key(input.candidate_id, input.job_id)],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .map_err(|error| error.to_string())?;

    let saved_questions = conn
        .query_row(
            "SELECT content_json FROM interview_kits WHERE id = ?1",
            [id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| error.to_string())
        .and_then(|content_text| {
            let content: Value =
                serde_json::from_str(&content_text).map_err(|error| error.to_string())?;
            serde_json::from_value::<Vec<InterviewQuestion>>(
                content.get("questions").cloned().unwrap_or(Value::Null),
            )
            .map_err(|error| error.to_string())
        })?;

    let record = InterviewKitRecord {
        id: Some(id),
        candidate_id: input.candidate_id,
        job_id: input.job_id,
        questions: saved_questions,
        generated_by: saved_generated_by,
        created_at,
        updated_at,
    };

    write_audit(
        &conn,
        "interview.kit.save",
        "interview_kit",
        Some(id.to_string()),
        serde_json::json!({
            "candidateId": record.candidate_id,
            "jobId": record.job_id,
            "questionCount": record.questions.len(),
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(record)
}

#[tauri::command]
fn submit_interview_feedback(
    state: State<'_, AppState>,
    input: SubmitInterviewFeedbackInput,
) -> Result<InterviewFeedbackRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    conn.query_row(
        "SELECT id FROM candidates WHERE id = ?1",
        [input.candidate_id],
        |row| row.get::<_, i64>(0),
    )
    .optional()
    .map_err(|error| error.to_string())?
    .ok_or_else(|| format!("Candidate {} not found", input.candidate_id))?;

    let transcript_text = input.transcript_text.trim().to_string();
    if transcript_text.is_empty() {
        return Err("interview_transcript_required".to_string());
    }
    if !input.structured_feedback.is_object() {
        return Err("interview_structured_feedback_required".to_string());
    }

    let recording_path = input
        .recording_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let now = now_iso();
    conn.execute(
        r#"
        INSERT INTO interview_feedback(
            candidate_id, job_id, transcript_text, structured_feedback_json,
            recording_path, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            input.candidate_id,
            input.job_id,
            transcript_text,
            input.structured_feedback.to_string(),
            recording_path,
            now,
            now,
        ],
    )
    .map_err(|error| error.to_string())?;

    let id = conn.last_insert_rowid();
    let record = conn
        .query_row(
            r#"
            SELECT id, candidate_id, job_id, transcript_text, structured_feedback_json,
                   recording_path, created_at, updated_at
            FROM interview_feedback
            WHERE id = ?1
            "#,
            [id],
            |row| {
                let structured_text: String = row.get(4)?;
                let structured_feedback =
                    serde_json::from_str(&structured_text).unwrap_or(Value::Object(Default::default()));
                Ok(InterviewFeedbackRecord {
                    id: row.get(0)?,
                    candidate_id: row.get(1)?,
                    job_id: row.get(2)?,
                    transcript_text: row.get(3)?,
                    structured_feedback,
                    recording_path: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        )
        .map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "interview.feedback.submit",
        "interview_feedback",
        Some(record.id.to_string()),
        serde_json::json!({
            "candidateId": record.candidate_id,
            "jobId": record.job_id,
            "transcriptLength": record.transcript_text.chars().count(),
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(record)
}

#[tauri::command]
fn run_interview_evaluation(
    state: State<'_, AppState>,
    input: RunInterviewEvaluationInput,
) -> Result<InterviewEvaluationRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;

    let parse_feedback_row = |row: &rusqlite::Row<'_>| -> Result<(i64, i64, Option<i64>, String, Value), rusqlite::Error> {
        let structured_text: String = row.get(4)?;
        let structured_feedback = serde_json::from_str(&structured_text).map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(err),
            )
        })?;
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Option<i64>>(2)?,
            row.get::<_, String>(3)?,
            structured_feedback,
        ))
    };

    let feedback = if let Some(feedback_id) = input.feedback_id {
        conn.query_row(
            r#"
            SELECT id, candidate_id, job_id, transcript_text, structured_feedback_json
            FROM interview_feedback
            WHERE id = ?1
            "#,
            [feedback_id],
            parse_feedback_row,
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("interview_feedback {} not found", feedback_id))?
    } else if let Some(job_id) = input.job_id {
        conn.query_row(
            r#"
            SELECT id, candidate_id, job_id, transcript_text, structured_feedback_json
            FROM interview_feedback
            WHERE candidate_id = ?1 AND job_id = ?2
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            params![input.candidate_id, job_id],
            parse_feedback_row,
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "interview_feedback_not_found".to_string())?
    } else {
        conn.query_row(
            r#"
            SELECT id, candidate_id, job_id, transcript_text, structured_feedback_json
            FROM interview_feedback
            WHERE candidate_id = ?1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            [input.candidate_id],
            parse_feedback_row,
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "interview_feedback_not_found".to_string())?
    };

    if feedback.1 != input.candidate_id {
        return Err("feedback_candidate_mismatch".to_string());
    }

    let payload = evaluate_interview_feedback_payload(&feedback.3, &feedback.4);
    let effective_job_id = input.job_id.or(feedback.2);
    let created_at = now_iso();
    conn.execute(
        r#"
        INSERT INTO interview_evaluations(
            candidate_id, job_id, feedback_id, recommendation, overall_score, confidence,
            evidence_json, verification_points_json, uncertainty, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            input.candidate_id,
            effective_job_id,
            feedback.0,
            payload.recommendation,
            payload.overall_score,
            payload.confidence,
            serde_json::to_string(&payload.evidence).map_err(|error| error.to_string())?,
            serde_json::to_string(&payload.verification_points).map_err(|error| error.to_string())?,
            payload.uncertainty,
            created_at,
        ],
    )
    .map_err(|error| error.to_string())?;

    let id = conn.last_insert_rowid();
    let record = InterviewEvaluationRecord {
        id,
        candidate_id: input.candidate_id,
        job_id: effective_job_id,
        feedback_id: feedback.0,
        recommendation: payload.recommendation.clone(),
        overall_score: payload.overall_score,
        confidence: payload.confidence,
        evidence: payload.evidence.clone(),
        verification_points: payload.verification_points.clone(),
        uncertainty: payload.uncertainty.clone(),
        created_at,
    };

    write_audit(
        &conn,
        "interview.evaluation.run",
        "interview_evaluation",
        Some(record.id.to_string()),
        serde_json::json!({
            "candidateId": record.candidate_id,
            "jobId": record.job_id,
            "feedbackId": record.feedback_id,
            "recommendation": record.recommendation,
            "overallScore": record.overall_score,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(record)
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

fn build_fts_match_query(input: &str) -> Option<String> {
    let token_regex = Regex::new(r"[\p{L}\p{N}_]+").ok()?;
    let tokens = token_regex
        .find_iter(input)
        .map(|item| item.as_str().to_lowercase())
        .filter(|item| !item.is_empty())
        .take(8)
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return None;
    }

    Some(
        tokens
            .into_iter()
            .map(|token| format!("\"{token}\"*"))
            .collect::<Vec<_>>()
            .join(" AND "),
    )
}

#[tauri::command]
fn search_candidates(state: State<'_, AppState>, query: String) -> Result<Vec<SearchHit>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let Some(match_query) = build_fts_match_query(&query) else {
        return Ok(Vec::new());
    };

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
        .query_map([match_query], |row| {
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
            "SELECT COUNT(*) FROM crawl_tasks WHERE status IN ('PENDING', 'RUNNING', 'PAUSED')",
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

#[tauri::command]
fn ensure_sidecar(state: State<'_, AppState>) -> Result<SidecarRuntime, String> {
    ensure_sidecar_running(state.inner())
}

fn resolve_db_path(app: &AppHandle) -> AppResult<PathBuf> {
    let data_dir = app.path().app_data_dir().map_err(|_| AppError::NotFound("app_data_dir".to_string()))?;
    fs::create_dir_all(&data_dir)?;
    Ok(data_dir.join("doss.sqlite3"))
}

fn normalize_local_key(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|trimmed| !trimmed.is_empty())
}

fn generate_system_local_key() -> String {
    let bytes: [u8; 32] = rand::random();
    BASE64_STANDARD.encode(bytes)
}

fn resolve_local_key(app: &AppHandle, value: Option<String>) -> AppResult<String> {
    if let Some(key) = normalize_local_key(value) {
        return Ok(key);
    }

    let data_dir = app.path().app_data_dir().map_err(|_| AppError::NotFound("app_data_dir".to_string()))?;
    fs::create_dir_all(&data_dir)?;
    let key_path = data_dir.join("doss.local.key");

    match fs::read_to_string(&key_path) {
        Ok(existing) => {
            if let Some(key) = normalize_local_key(Some(existing)) {
                return Ok(key);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(AppError::Io(error)),
    }

    let generated = generate_system_local_key();
    fs::write(&key_path, format!("{generated}\n"))?;
    Ok(generated)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let db_path = resolve_db_path(app.handle())?;
            migrate_db(&db_path)?;

            let seed = resolve_local_key(app.handle(), std::env::var("DOSS_LOCAL_KEY").ok())?;

            let preferred_sidecar_port = std::env::var("CRAWLER_PORT")
                .ok()
                .and_then(|value| value.parse::<u16>().ok())
                .unwrap_or(3791);
            let sidecar_command = std::env::var("DOSS_SIDECAR_CMD")
                .unwrap_or_else(|_| "pnpm --filter @doss/crawler-sidecar dev".to_string());
            let sidecar_cwd = std::env::var("DOSS_SIDECAR_CWD")
                .ok()
                .map(PathBuf::from)
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

            let state = AppState::new(
                db_path,
                &seed,
                sidecar_command,
                sidecar_cwd,
                preferred_sidecar_port,
            );

            let sidecar_autostart = std::env::var("DOSS_SIDECAR_AUTOSTART")
                .ok()
                .map(|value| value.trim().to_lowercase())
                .map(|value| !matches!(value.as_str(), "0" | "false" | "no"))
                .unwrap_or(true);

            if sidecar_autostart {
                let _ = ensure_sidecar_running(&state);
            }

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_health,
            ensure_sidecar,
            create_job,
            list_jobs,
            create_candidate,
            merge_candidate_import,
            list_candidates,
            move_candidate_stage,
            list_pipeline_events,
            upsert_resume,
            parse_resume_file,
            get_screening_template,
            upsert_screening_template,
            run_resume_screening,
            list_screening_results,
            generate_interview_kit,
            save_interview_kit,
            submit_interview_feedback,
            run_interview_evaluation,
            get_ai_provider_catalog,
            list_ai_provider_profiles,
            upsert_ai_provider_profile,
            delete_ai_provider_profile,
            set_default_ai_provider_profile,
            test_ai_provider_profile,
            get_ai_provider_settings,
            upsert_ai_provider_settings,
            test_ai_provider_settings,
            get_task_runtime_settings,
            upsert_task_runtime_settings,
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

    #[test]
    fn extract_json_object_block_works_for_markdown_wrapped_json() {
        let text = "模型输出如下:\n```json\n{\"overall_score\":88,\"dimension_scores\":[]}\n```";
        let extracted = extract_json_object_block(text).expect("extract json");
        assert_eq!(extracted, "{\"overall_score\":88,\"dimension_scores\":[]}");
    }

    #[test]
    fn parse_ai_provider_response_accepts_camel_case_keys() {
        let text = r#"{
          "overallScore": 91,
          "dimensionScores": [
            { "key": "skill_match", "score": 90, "reason": "技能匹配好" },
            { "key": "experience", "score": 88, "reason": "年限满足" },
            { "key": "compensation", "score": 85, "reason": "预算匹配" },
            { "key": "stability", "score": 84, "reason": "稳定性正常" }
          ],
          "risks": ["需确认业务领域经验"],
          "highlights": ["核心技能覆盖充分"],
          "suggestions": ["安排技术面"],
          "evidence": [
            {
              "dimension": "skill_match",
              "statement": "命中 Vue3 / TypeScript",
              "sourceSnippet": "候选人技能: Vue3, TypeScript"
            }
          ],
          "confidence": 0.86
        }"#;

        let parsed = parse_ai_provider_response(text).expect("parse provider response");
        assert_eq!(parsed.overall_score, 91);
        assert_eq!(parsed.dimension_scores.len(), 4);
        assert_eq!(parsed.evidence.len(), 1);
        assert_eq!(parsed.confidence, Some(0.86));
    }

    #[test]
    fn sidecar_port_candidates_include_preferred_and_fallback() {
        let ports = sidecar_port_candidates(3791);
        assert_eq!(ports[0], 3791);
        assert_eq!(ports.len(), 6);
        assert!(ports.contains(&3792));
        assert!(ports.contains(&3796));
    }

    #[test]
    fn sidecar_base_url_uses_localhost() {
        assert_eq!(sidecar_base_url(3791), "http://127.0.0.1:3791");
    }

    #[test]
    fn normalize_local_key_uses_trimmed_env_value() {
        assert_eq!(
            normalize_local_key(Some(" test-secret ".to_string())),
            Some("test-secret".to_string())
        );
        assert_eq!(normalize_local_key(Some("   ".to_string())), None);
        assert_eq!(normalize_local_key(None), None);
    }

    #[test]
    fn generate_system_local_key_returns_random_non_empty_value() {
        let first = generate_system_local_key();
        let second = generate_system_local_key();

        assert!(!first.trim().is_empty());
        assert!(!second.trim().is_empty());
        assert!(first.len() >= 40);
        assert!(second.len() >= 40);
        assert_ne!(first, second);
    }

    #[test]
    fn build_fts_match_query_sanitizes_special_characters() {
        assert_eq!(build_fts_match_query("\""), None);
        assert_eq!(
            build_fts_match_query("Vue3 (TypeScript)"),
            Some("\"vue3\"* AND \"typescript\"*".to_string())
        );
        assert_eq!(
            build_fts_match_query("前端 \"工程师\""),
            Some("\"前端\"* AND \"工程师\"*".to_string())
        );
    }

    #[test]
    fn extract_docx_xml_text_collects_runs() {
        let xml = r#"<w:document><w:body><w:p><w:r><w:t>张三</w:t></w:r><w:r><w:t> 5年Vue开发经验 </w:t></w:r></w:p></w:body></w:document>"#;
        let text = extract_docx_xml_text(xml.as_bytes()).expect("extract docx xml");
        assert!(text.contains("张三"));
        assert!(text.contains("5年Vue开发经验"));
    }

    #[test]
    fn build_structured_resume_fields_extracts_skills_and_salary() {
        let raw_text = "候选人熟悉 Vue3 / TypeScript / Playwright，8年经验，期望薪资 45k";
        let parsed = build_structured_resume_fields(raw_text);

        let skills = parsed
            .get("skills")
            .and_then(|value| value.as_array())
            .expect("skills");
        assert!(skills.iter().any(|value| value.as_str() == Some("Vue3")));
        assert!(skills.iter().any(|value| value.as_str() == Some("TypeScript")));

        let expected_salary = parsed
            .get("expectedSalaryK")
            .and_then(|value| value.as_f64())
            .expect("expectedSalaryK");
        assert_eq!(expected_salary, 45.0);
    }

    #[test]
    fn normalize_task_runtime_settings_clamps_values() {
        let normalized = normalize_task_runtime_settings(TaskRuntimeSettings {
            auto_batch_concurrency: 99,
            auto_retry_count: -10,
            auto_retry_backoff_ms: 20,
        });
        assert_eq!(normalized.auto_batch_concurrency, 8);
        assert_eq!(normalized.auto_retry_count, 0);
        assert_eq!(normalized.auto_retry_backoff_ms, 100);
    }

    #[test]
    fn normalize_screening_dimensions_requires_weight_sum_100() {
        let result = normalize_screening_dimensions(Some(vec![
            ScreeningDimension {
                key: "a".to_string(),
                label: "A".to_string(),
                weight: 60,
            },
            ScreeningDimension {
                key: "b".to_string(),
                label: "B".to_string(),
                weight: 20,
            },
        ]));

        assert!(result.is_err());
    }

    #[test]
    fn normalize_screening_dimensions_returns_default_when_empty_input() {
        let dimensions = normalize_screening_dimensions(None).expect("default dimensions");
        let total = dimensions.iter().map(|item| item.weight).sum::<i32>();
        assert_eq!(total, 100);
        assert!(dimensions.len() >= 4);
    }

    #[test]
    fn interview_evaluation_returns_hold_when_evidence_insufficient() {
        let payload = evaluate_interview_feedback_payload(
            "候选人简单介绍，暂无详细问答。",
            &serde_json::json!({
                "scores": {
                    "communication": 3.5
                },
                "summary": "仅完成了短时沟通"
            }),
        );

        assert_eq!(payload.recommendation, "HOLD");
        assert!(payload.verification_points.iter().any(|item| item.contains("补充")));
    }

    #[test]
    fn ai_provider_catalog_contains_official_providers() {
        let providers = AiProvider::all()
            .iter()
            .map(AiProvider::to_catalog_item)
            .collect::<Vec<_>>();
        let ids = providers
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            ids,
            vec!["qwen", "doubao", "deepseek", "minimax", "glm", "openapi"]
        );
        assert!(
            providers
                .iter()
                .all(|item| !item.default_model.trim().is_empty()
                    && !item.default_base_url.trim().is_empty()
                    && !item.models.is_empty()
                    && !item.docs.is_empty())
        );
    }

    #[test]
    fn parse_minimax_content_handles_openai_style_choices() {
        let body = serde_json::json!({
            "base_resp": {"status_code": 0, "status_msg": "success"},
            "choices": [
                {"message": {"role": "assistant", "content": "OK"}}
            ]
        });

        let parsed = parse_minimax_content(&body).expect("parse minimax content");
        assert_eq!(parsed, "OK");
    }

    #[test]
    fn parse_minimax_content_surfaces_business_error() {
        let body = serde_json::json!({
            "base_resp": {"status_code": 1004, "status_msg": "login fail"}
        });

        let error = parse_minimax_content(&body).expect_err("should fail");
        assert_eq!(error, "provider_api_error_1004: login fail");
    }

    #[test]
    fn ai_provider_from_db_migrates_legacy_mock() {
        assert_eq!(AiProvider::from_db("mock"), AiProvider::Qwen);
        assert_eq!(
            AiProvider::from_db("openai-compatible"),
            AiProvider::OpenApi
        );
        assert_eq!(
            AiProvider::from_db("openai_compatible"),
            AiProvider::OpenApi
        );
        assert_eq!(
            AiProvider::from_db("openapi-compatible"),
            AiProvider::OpenApi
        );
    }
}
