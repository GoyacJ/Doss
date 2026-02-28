use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ScoringItemConfig {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) description: String,
    pub(crate) weight: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ScoringSectionConfig {
    pub(crate) items: Vec<ScoringItemConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ScoringWeights {
    pub(crate) t0: i32,
    pub(crate) t1: i32,
    pub(crate) t2: i32,
    pub(crate) t3: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ScoringTemplateConfig {
    pub(crate) weights: ScoringWeights,
    pub(crate) t0: ScoringSectionConfig,
    pub(crate) t1: ScoringSectionConfig,
    pub(crate) t2: ScoringSectionConfig,
    pub(crate) t3: ScoringSectionConfig,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ScoringTemplateRecord {
    pub(crate) id: i64,
    pub(crate) scope: String,
    pub(crate) name: String,
    pub(crate) job_id: Option<i64>,
    pub(crate) config: ScoringTemplateConfig,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct UpsertScoringTemplateInput {
    pub(crate) job_id: Option<i64>,
    pub(crate) name: Option<String>,
    pub(crate) config: Option<ScoringTemplateConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CreateScoringTemplateInput {
    pub(crate) name: Option<String>,
    pub(crate) config: Option<ScoringTemplateConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct UpdateScoringTemplateInput {
    pub(crate) template_id: i64,
    pub(crate) name: Option<String>,
    pub(crate) config: Option<ScoringTemplateConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SetJobScoringTemplateInput {
    pub(crate) job_id: i64,
    pub(crate) template_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RunCandidateScoringInput {
    pub(crate) candidate_id: i64,
    pub(crate) job_id: Option<i64>,
    pub(crate) run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ScoringResultRecord {
    pub(crate) id: i64,
    pub(crate) candidate_id: i64,
    pub(crate) job_id: Option<i64>,
    pub(crate) template_id: Option<i64>,
    pub(crate) overall_score: i32,
    pub(crate) overall_score_5: f64,
    pub(crate) t0_score_5: f64,
    pub(crate) t1_score_5: f64,
    pub(crate) t2_score_5: f64,
    pub(crate) t3_score_5: f64,
    pub(crate) recommendation: String,
    pub(crate) risk_level: String,
    pub(crate) structured_result: Value,
    pub(crate) created_at: String,
}
