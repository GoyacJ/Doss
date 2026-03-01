use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::common::{PageQuery, PipelineStage, SourceType};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Candidate {
    pub(crate) id: i64,
    pub(crate) external_id: Option<String>,
    pub(crate) source: String,
    pub(crate) name: String,
    pub(crate) current_company: Option<String>,
    pub(crate) job_id: Option<i64>,
    pub(crate) job_title: Option<String>,
    pub(crate) score: Option<f64>,
    pub(crate) age: Option<i32>,
    pub(crate) gender: Option<String>,
    pub(crate) years_of_experience: f64,
    pub(crate) address: Option<String>,
    pub(crate) stage: PipelineStage,
    pub(crate) tags: Vec<String>,
    pub(crate) phone_masked: Option<String>,
    pub(crate) email_masked: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct NewCandidateInput {
    pub(crate) external_id: Option<String>,
    pub(crate) source: Option<SourceType>,
    pub(crate) name: String,
    pub(crate) current_company: Option<String>,
    pub(crate) score: Option<f64>,
    pub(crate) age: Option<i32>,
    pub(crate) gender: Option<String>,
    pub(crate) years_of_experience: f64,
    pub(crate) address: Option<String>,
    pub(crate) phone: Option<String>,
    pub(crate) email: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) job_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct UpdateCandidateInput {
    pub(crate) candidate_id: i64,
    pub(crate) name: String,
    pub(crate) current_company: Option<String>,
    pub(crate) job_id: Option<i64>,
    pub(crate) score: Option<f64>,
    pub(crate) age: Option<i32>,
    pub(crate) gender: Option<String>,
    pub(crate) years_of_experience: f64,
    pub(crate) address: Option<String>,
    pub(crate) phone: Option<String>,
    pub(crate) email: Option<String>,
    pub(crate) tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct MergeCandidateImportInput {
    pub(crate) candidate_id: i64,
    pub(crate) current_company: Option<String>,
    pub(crate) years_of_experience: Option<f64>,
    pub(crate) address: Option<String>,
    pub(crate) phone: Option<String>,
    pub(crate) email: Option<String>,
    pub(crate) tags: Option<Vec<String>>,
    pub(crate) job_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ResumeRecord {
    pub(crate) id: i64,
    pub(crate) candidate_id: i64,
    pub(crate) source: String,
    pub(crate) raw_text: String,
    pub(crate) parsed: Value,
    pub(crate) original_file_name: Option<String>,
    pub(crate) original_file_content_type: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ResumeOriginalFileInput {
    pub(crate) file_name: String,
    pub(crate) content_base64: String,
    pub(crate) content_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct UpsertResumeInput {
    pub(crate) candidate_id: i64,
    pub(crate) source: Option<SourceType>,
    pub(crate) raw_text: Option<String>,
    pub(crate) parsed: Option<Value>,
    pub(crate) enable_ocr: Option<bool>,
    pub(crate) original_file: Option<ResumeOriginalFileInput>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PipelineEvent {
    pub(crate) id: i64,
    pub(crate) candidate_id: i64,
    pub(crate) job_id: Option<i64>,
    pub(crate) from_stage: PipelineStage,
    pub(crate) to_stage: PipelineStage,
    pub(crate) note: Option<String>,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct MoveStageInput {
    pub(crate) candidate_id: i64,
    pub(crate) job_id: Option<i64>,
    pub(crate) to_stage: PipelineStage,
    pub(crate) note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SetCandidateQualificationInput {
    pub(crate) candidate_id: i64,
    pub(crate) qualified: bool,
    pub(crate) note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SortRule {
    pub(crate) field: String,
    pub(crate) direction: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CandidateListQuery {
    #[serde(flatten)]
    pub(crate) page: PageQuery,
    pub(crate) job_id: Option<i64>,
    pub(crate) name_like: Option<String>,
    pub(crate) stage: Option<PipelineStage>,
    pub(crate) sorts: Option<Vec<SortRule>>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct InterviewListQuery {
    #[serde(flatten)]
    pub(crate) page: PageQuery,
    pub(crate) job_id: Option<i64>,
    pub(crate) name_like: Option<String>,
    pub(crate) sorts: Option<Vec<SortRule>>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DecisionListQuery {
    #[serde(flatten)]
    pub(crate) page: PageQuery,
    pub(crate) job_id: Option<i64>,
    pub(crate) name_like: Option<String>,
    pub(crate) interview_passed: Option<bool>,
    pub(crate) sorts: Option<Vec<SortRule>>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PendingCandidate {
    pub(crate) id: i64,
    pub(crate) source: String,
    pub(crate) external_id: Option<String>,
    pub(crate) name: String,
    pub(crate) current_company: Option<String>,
    pub(crate) job_id: Option<i64>,
    pub(crate) job_title: Option<String>,
    pub(crate) age: Option<i32>,
    pub(crate) gender: Option<String>,
    pub(crate) years_of_experience: f64,
    pub(crate) tags: Vec<String>,
    pub(crate) phone_masked: Option<String>,
    pub(crate) email_masked: Option<String>,
    pub(crate) address: Option<String>,
    pub(crate) extra_notes: Option<String>,
    pub(crate) resume_raw_text: Option<String>,
    pub(crate) resume_parsed: Value,
    pub(crate) dedupe_key: String,
    pub(crate) sync_status: String,
    pub(crate) sync_error_code: Option<String>,
    pub(crate) sync_error_message: Option<String>,
    pub(crate) candidate_id: Option<i64>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct UpsertPendingCandidateInput {
    pub(crate) source: Option<SourceType>,
    pub(crate) external_id: Option<String>,
    pub(crate) name: String,
    pub(crate) current_company: Option<String>,
    pub(crate) job_id: Option<i64>,
    pub(crate) age: Option<i32>,
    pub(crate) gender: Option<String>,
    pub(crate) years_of_experience: Option<f64>,
    pub(crate) tags: Option<Vec<String>>,
    pub(crate) phone: Option<String>,
    pub(crate) email: Option<String>,
    pub(crate) address: Option<String>,
    pub(crate) extra_notes: Option<String>,
    pub(crate) resume_raw_text: Option<String>,
    pub(crate) resume_parsed: Option<Value>,
    pub(crate) dedupe_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct UpsertPendingCandidatesInput {
    pub(crate) items: Vec<UpsertPendingCandidateInput>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct PendingCandidateListQuery {
    #[serde(flatten)]
    pub(crate) page: PageQuery,
    pub(crate) sync_status: Option<String>,
    pub(crate) name_like: Option<String>,
    pub(crate) job_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SyncPendingCandidateInput {
    pub(crate) pending_candidate_id: i64,
    pub(crate) run_screening: Option<bool>,
}
