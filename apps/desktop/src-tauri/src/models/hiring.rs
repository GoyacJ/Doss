use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct FinalizeHiringDecisionInput {
    pub(crate) candidate_id: i64,
    pub(crate) job_id: Option<i64>,
    pub(crate) final_decision: String,
    pub(crate) reason_code: String,
    pub(crate) note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HiringDecisionRecord {
    pub(crate) id: i64,
    pub(crate) candidate_id: i64,
    pub(crate) job_id: Option<i64>,
    pub(crate) interview_evaluation_id: Option<i64>,
    pub(crate) ai_recommendation: Option<String>,
    pub(crate) final_decision: String,
    pub(crate) reason_code: String,
    pub(crate) note: Option<String>,
    pub(crate) ai_deviation: bool,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}
