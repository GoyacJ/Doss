use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ScreeningDimension {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) weight: i32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ScreeningTemplateRecord {
    pub(crate) id: i64,
    pub(crate) scope: String,
    pub(crate) name: String,
    pub(crate) job_id: Option<i64>,
    pub(crate) dimensions: Vec<ScreeningDimension>,
    pub(crate) risk_rules: Value,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct UpsertScreeningTemplateInput {
    pub(crate) job_id: Option<i64>,
    pub(crate) name: Option<String>,
    pub(crate) dimensions: Option<Vec<ScreeningDimension>>,
    pub(crate) risk_rules: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CreateScreeningTemplateInput {
    pub(crate) name: Option<String>,
    pub(crate) dimensions: Option<Vec<ScreeningDimension>>,
    pub(crate) risk_rules: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct UpdateScreeningTemplateInput {
    pub(crate) template_id: i64,
    pub(crate) name: Option<String>,
    pub(crate) dimensions: Option<Vec<ScreeningDimension>>,
    pub(crate) risk_rules: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SetJobScreeningTemplateInput {
    pub(crate) job_id: i64,
    pub(crate) template_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RunScreeningInput {
    pub(crate) candidate_id: i64,
    pub(crate) job_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ScreeningResultRecord {
    pub(crate) id: i64,
    pub(crate) candidate_id: i64,
    pub(crate) job_id: Option<i64>,
    pub(crate) template_id: Option<i64>,
    pub(crate) t0_score: f64,
    pub(crate) t1_score: i32,
    pub(crate) fine_score: i32,
    pub(crate) bonus_score: i32,
    pub(crate) risk_penalty: i32,
    pub(crate) overall_score: i32,
    pub(crate) recommendation: String,
    pub(crate) risk_level: String,
    pub(crate) evidence: Vec<String>,
    pub(crate) verification_points: Vec<String>,
    pub(crate) created_at: String,
}
