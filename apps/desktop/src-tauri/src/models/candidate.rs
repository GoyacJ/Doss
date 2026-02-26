use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::common::{PipelineStage, SourceType};

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
    pub(crate) phone: Option<String>,
    pub(crate) email: Option<String>,
    pub(crate) tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct MergeCandidateImportInput {
    pub(crate) candidate_id: i64,
    pub(crate) current_company: Option<String>,
    pub(crate) years_of_experience: Option<f64>,
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
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct UpsertResumeInput {
    pub(crate) candidate_id: i64,
    pub(crate) source: Option<SourceType>,
    pub(crate) raw_text: String,
    pub(crate) parsed: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ParseResumeFileInput {
    pub(crate) file_name: String,
    pub(crate) content_base64: String,
    pub(crate) enable_ocr: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ParseResumeFileOutput {
    pub(crate) raw_text: String,
    pub(crate) parsed: Value,
    pub(crate) metadata: Value,
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
