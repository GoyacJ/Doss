use serde_json::Value;
use tauri::State;

use crate::core::state::AppState;
use crate::infra::db::open_connection;
use crate::models::common::PipelineStage;
use crate::models::metrics::{DashboardMetrics, StageStat};

#[tauri::command]
pub(crate) fn dashboard_metrics(state: State<'_, AppState>) -> Result<DashboardMetrics, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;

    let total_jobs: i64 = conn
        .query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    let total_candidates: i64 = conn
        .query_row("SELECT COUNT(*) FROM candidates", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    let total_resumes: i64 = conn
        .query_row("SELECT COUNT(*) FROM resumes", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    let pending_tasks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM crawl_tasks WHERE status IN ('PENDING', 'RUNNING', 'PAUSED')",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let hiring_decisions_total: i64 = conn
        .query_row("SELECT COUNT(*) FROM hiring_decisions", [], |row| {
            row.get(0)
        })
        .map_err(|error| error.to_string())?;
    let ai_deviation_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM hiring_decisions WHERE ai_deviation = 1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let ai_alignment_count = hiring_decisions_total.saturating_sub(ai_deviation_count);
    let ai_alignment_rate = if hiring_decisions_total > 0 {
        ((ai_alignment_count as f64 / hiring_decisions_total as f64) * 1000.0).round() / 10.0
    } else {
        0.0
    };

    let mut stage_stmt = conn
        .prepare("SELECT stage, COUNT(*) FROM candidates GROUP BY stage")
        .map_err(|error| error.to_string())?;
    let stage_rows = stage_stmt
        .query_map([], |row| {
            let stage_text: String = row.get(0)?;
            Ok(StageStat {
                stage: PipelineStage::from_db(&stage_text).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?,
                count: row.get(1)?,
            })
        })
        .map_err(|error| error.to_string())?;

    let stage_stats = stage_rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(DashboardMetrics {
        total_jobs,
        total_candidates,
        total_resumes,
        pending_tasks,
        hiring_decisions_total,
        ai_alignment_count,
        ai_deviation_count,
        ai_alignment_rate,
        stage_stats,
    })
}

#[tauri::command]
pub(crate) fn app_health(state: State<'_, AppState>) -> Result<Value, String> {
    let db_exists = state.db_path.exists();
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let schema_version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;

    Ok(serde_json::json!({
        "ok": true,
        "dbPath": state.db_path,
        "dbExists": db_exists,
        "schemaVersion": schema_version,
    }))
}
