use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::common::{CrawlMode, CrawlTaskStatus, SourceType};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CrawlTask {
    pub(crate) id: i64,
    pub(crate) source: String,
    pub(crate) mode: String,
    pub(crate) task_type: String,
    pub(crate) status: String,
    pub(crate) retry_count: i32,
    pub(crate) error_code: Option<String>,
    pub(crate) payload: Value,
    pub(crate) snapshot: Option<Value>,
    pub(crate) started_at: Option<String>,
    pub(crate) finished_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct NewCrawlTaskInput {
    pub(crate) source: SourceType,
    pub(crate) mode: CrawlMode,
    pub(crate) task_type: String,
    pub(crate) payload: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct UpdateCrawlTaskInput {
    pub(crate) task_id: i64,
    pub(crate) status: CrawlTaskStatus,
    pub(crate) retry_count: Option<i32>,
    pub(crate) error_code: Option<String>,
    pub(crate) snapshot: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CrawlTaskPerson {
    pub(crate) id: i64,
    pub(crate) task_id: i64,
    pub(crate) source: String,
    pub(crate) external_id: Option<String>,
    pub(crate) name: String,
    pub(crate) current_company: Option<String>,
    pub(crate) years_of_experience: f64,
    pub(crate) sync_status: String,
    pub(crate) sync_error_code: Option<String>,
    pub(crate) sync_error_message: Option<String>,
    pub(crate) candidate_id: Option<i64>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct UpsertCrawlTaskPersonInput {
    pub(crate) source: SourceType,
    pub(crate) external_id: Option<String>,
    pub(crate) name: String,
    pub(crate) current_company: Option<String>,
    pub(crate) years_of_experience: f64,
    pub(crate) sync_status: Option<String>,
    pub(crate) sync_error_code: Option<String>,
    pub(crate) sync_error_message: Option<String>,
    pub(crate) candidate_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct UpsertCrawlTaskPeopleInput {
    pub(crate) task_id: i64,
    pub(crate) people: Vec<UpsertCrawlTaskPersonInput>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CrawlTaskPersonSyncUpdate {
    pub(crate) person_id: i64,
    pub(crate) sync_status: String,
    pub(crate) sync_error_code: Option<String>,
    pub(crate) sync_error_message: Option<String>,
    pub(crate) candidate_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct UpdateCrawlTaskPeopleSyncInput {
    pub(crate) task_id: i64,
    pub(crate) updates: Vec<CrawlTaskPersonSyncUpdate>,
}
