use serde::{Deserialize, Serialize};

use crate::models::common::SourceType;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Job {
    pub(crate) id: i64,
    pub(crate) external_id: Option<String>,
    pub(crate) source: String,
    pub(crate) title: String,
    pub(crate) company: String,
    pub(crate) city: Option<String>,
    pub(crate) salary_k: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) status: String,
    pub(crate) screening_template_id: Option<i64>,
    pub(crate) screening_template_name: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct NewJobInput {
    pub(crate) external_id: Option<String>,
    pub(crate) source: Option<SourceType>,
    pub(crate) title: String,
    pub(crate) company: String,
    pub(crate) city: Option<String>,
    pub(crate) salary_k: Option<String>,
    pub(crate) description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct UpdateJobInput {
    pub(crate) job_id: i64,
    pub(crate) title: String,
    pub(crate) company: String,
    pub(crate) city: Option<String>,
    pub(crate) salary_k: Option<String>,
    pub(crate) description: Option<String>,
}
