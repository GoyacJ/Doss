use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::common::AiProvider;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AiProviderCatalogItem {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) default_model: String,
    pub(crate) default_base_url: String,
    pub(crate) models: Vec<String>,
    pub(crate) docs: Vec<String>,
}

impl AiProvider {
    pub(crate) fn to_catalog_item(&self) -> AiProviderCatalogItem {
        AiProviderCatalogItem {
            id: self.as_db().to_string(),
            label: self.label().to_string(),
            default_model: self.default_model().to_string(),
            default_base_url: self.default_base_url().to_string(),
            models: self
                .models()
                .iter()
                .map(|item| (*item).to_string())
                .collect(),
            docs: self.docs().iter().map(|item| (*item).to_string()).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AiProviderCatalogView {
    pub(crate) providers: Vec<AiProviderCatalogItem>,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StoredAiProviderSettings {
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) base_url: String,
    pub(crate) api_key_enc: Option<String>,
    pub(crate) temperature: f64,
    pub(crate) max_tokens: i32,
    pub(crate) timeout_secs: i32,
    pub(crate) retry_count: i32,
}

impl StoredAiProviderSettings {
    pub(crate) fn defaults(provider: AiProvider) -> Self {
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
pub(crate) struct ResolvedAiProviderSettings {
    pub(crate) provider: AiProvider,
    pub(crate) model: String,
    pub(crate) base_url: String,
    pub(crate) api_key: Option<String>,
    pub(crate) temperature: f64,
    pub(crate) max_tokens: i32,
    pub(crate) timeout_secs: i32,
    pub(crate) retry_count: i32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AiProviderSettingsView {
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) base_url: String,
    pub(crate) temperature: f64,
    pub(crate) max_tokens: i32,
    pub(crate) timeout_secs: i32,
    pub(crate) retry_count: i32,
    pub(crate) has_api_key: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AiProviderTestResult {
    pub(crate) ok: bool,
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) endpoint: String,
    pub(crate) latency_ms: u64,
    pub(crate) reply_excerpt: String,
    pub(crate) tested_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct UpsertAiProviderSettingsInput {
    pub(crate) provider: String,
    pub(crate) model: Option<String>,
    pub(crate) base_url: Option<String>,
    pub(crate) temperature: Option<f64>,
    pub(crate) max_tokens: Option<i32>,
    pub(crate) timeout_secs: Option<i32>,
    pub(crate) retry_count: Option<i32>,
    pub(crate) api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StoredAiProviderProfile {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) base_url: String,
    pub(crate) api_key_enc: Option<String>,
    pub(crate) temperature: f64,
    pub(crate) max_tokens: i32,
    pub(crate) timeout_secs: i32,
    pub(crate) retry_count: i32,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StoredAiProviderProfiles {
    pub(crate) active_profile_id: String,
    pub(crate) profiles: Vec<StoredAiProviderProfile>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AiProviderProfileView {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) base_url: String,
    pub(crate) temperature: f64,
    pub(crate) max_tokens: i32,
    pub(crate) timeout_secs: i32,
    pub(crate) retry_count: i32,
    pub(crate) has_api_key: bool,
    pub(crate) is_active: bool,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct UpsertAiProviderProfileInput {
    pub(crate) profile_id: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) provider: String,
    pub(crate) model: Option<String>,
    pub(crate) base_url: Option<String>,
    pub(crate) temperature: Option<f64>,
    pub(crate) max_tokens: Option<i32>,
    pub(crate) timeout_secs: Option<i32>,
    pub(crate) retry_count: Option<i32>,
    pub(crate) api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DimensionScore {
    pub(crate) key: String,
    pub(crate) score: i32,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct EvidenceItem {
    pub(crate) dimension: String,
    pub(crate) statement: String,
    pub(crate) source_snippet: String,
}

#[derive(Debug, Clone)]
pub(crate) struct AiPromptContext {
    pub(crate) required_skills: Vec<String>,
    pub(crate) extracted_skills: Vec<String>,
    pub(crate) candidate_years: f64,
    pub(crate) expected_salary_k: Option<f64>,
    pub(crate) max_salary_k: Option<f64>,
    pub(crate) stage: String,
    pub(crate) tags: Vec<String>,
    pub(crate) resume_raw_text: String,
    pub(crate) resume_parsed: Value,
}

#[derive(Debug, Clone)]
pub(crate) struct AiAnalysisPayload {
    pub(crate) overall_score: i32,
    pub(crate) dimension_scores: Vec<DimensionScore>,
    pub(crate) risks: Vec<String>,
    pub(crate) highlights: Vec<String>,
    pub(crate) suggestions: Vec<String>,
    pub(crate) evidence: Vec<EvidenceItem>,
    pub(crate) confidence: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AnalysisRecord {
    pub(crate) id: i64,
    pub(crate) candidate_id: i64,
    pub(crate) job_id: Option<i64>,
    pub(crate) overall_score: i32,
    pub(crate) dimension_scores: Vec<DimensionScore>,
    pub(crate) risks: Vec<String>,
    pub(crate) highlights: Vec<String>,
    pub(crate) suggestions: Vec<String>,
    pub(crate) evidence: Vec<EvidenceItem>,
    pub(crate) model_info: Value,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RunAnalysisInput {
    pub(crate) candidate_id: i64,
    pub(crate) job_id: Option<i64>,
    pub(crate) run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TaskRuntimeSettings {
    pub(crate) auto_batch_concurrency: i32,
    pub(crate) auto_retry_count: i32,
    pub(crate) auto_retry_backoff_ms: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct UpsertTaskRuntimeSettingsInput {
    pub(crate) auto_batch_concurrency: Option<i32>,
    pub(crate) auto_retry_count: Option<i32>,
    pub(crate) auto_retry_backoff_ms: Option<i32>,
}
