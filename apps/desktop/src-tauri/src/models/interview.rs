use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct InterviewQuestion {
    pub(crate) primary_question: String,
    pub(crate) follow_ups: Vec<String>,
    pub(crate) scoring_points: Vec<String>,
    pub(crate) red_flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct InterviewKitRecord {
    pub(crate) id: Option<i64>,
    pub(crate) candidate_id: i64,
    pub(crate) job_id: Option<i64>,
    pub(crate) questions: Vec<InterviewQuestion>,
    pub(crate) generated_by: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct GenerateInterviewKitInput {
    pub(crate) candidate_id: i64,
    pub(crate) job_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SaveInterviewKitInput {
    pub(crate) candidate_id: i64,
    pub(crate) job_id: Option<i64>,
    pub(crate) questions: Vec<InterviewQuestion>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct InterviewFeedbackRecord {
    pub(crate) id: i64,
    pub(crate) candidate_id: i64,
    pub(crate) job_id: Option<i64>,
    pub(crate) transcript_text: String,
    pub(crate) structured_feedback: Value,
    pub(crate) recording_path: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SubmitInterviewFeedbackInput {
    pub(crate) candidate_id: i64,
    pub(crate) job_id: Option<i64>,
    pub(crate) transcript_text: String,
    pub(crate) structured_feedback: Value,
    pub(crate) recording_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RunInterviewEvaluationInput {
    pub(crate) candidate_id: i64,
    pub(crate) job_id: Option<i64>,
    pub(crate) feedback_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct InterviewEvaluationRecord {
    pub(crate) id: i64,
    pub(crate) candidate_id: i64,
    pub(crate) job_id: Option<i64>,
    pub(crate) feedback_id: i64,
    pub(crate) recommendation: String,
    pub(crate) overall_score: i32,
    pub(crate) confidence: f64,
    pub(crate) evidence: Vec<String>,
    pub(crate) verification_points: Vec<String>,
    pub(crate) uncertainty: String,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone)]
pub(crate) struct InterviewEvaluationPayload {
    pub(crate) recommendation: String,
    pub(crate) overall_score: i32,
    pub(crate) confidence: f64,
    pub(crate) evidence: Vec<String>,
    pub(crate) verification_points: Vec<String>,
    pub(crate) uncertainty: String,
}
