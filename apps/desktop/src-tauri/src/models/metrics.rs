use serde::Serialize;

use crate::models::common::PipelineStage;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct StageStat {
    pub(crate) stage: PipelineStage,
    pub(crate) count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DashboardMetrics {
    pub(crate) total_jobs: i64,
    pub(crate) total_candidates: i64,
    pub(crate) total_resumes: i64,
    pub(crate) pending_tasks: i64,
    pub(crate) hiring_decisions_total: i64,
    pub(crate) ai_alignment_count: i64,
    pub(crate) ai_deviation_count: i64,
    pub(crate) ai_alignment_rate: f64,
    pub(crate) stage_stats: Vec<StageStat>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SearchHit {
    pub(crate) candidate_id: i64,
    pub(crate) name: String,
    pub(crate) stage: PipelineStage,
    pub(crate) snippet: String,
}
