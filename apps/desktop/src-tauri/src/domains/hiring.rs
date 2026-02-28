use rusqlite::{params, OptionalExtension};
use tauri::State;

use crate::core::error::AppError;
use crate::core::state::AppState;
use crate::core::time::now_iso;
use crate::domains::recruiting_utils::{
    map_ai_recommendation_to_final_decision, normalize_final_decision,
};
use crate::infra::audit::write_audit;
use crate::infra::db::open_connection;
use crate::models::common::{is_valid_transition, PipelineStage};
use crate::models::hiring::{FinalizeHiringDecisionInput, HiringDecisionRecord};

#[tauri::command]
pub(crate) fn finalize_hiring_decision(
    state: State<'_, AppState>,
    input: FinalizeHiringDecisionInput,
) -> Result<HiringDecisionRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;

    let current_stage_text: String = conn
        .query_row(
            "SELECT stage FROM candidates WHERE id = ?1",
            [input.candidate_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Candidate {} not found", input.candidate_id))?;

    let reason_code = input.reason_code.trim().to_string();
    if reason_code.is_empty() {
        return Err("hiring_decision_reason_code_required".to_string());
    }
    let note = input
        .note
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let final_decision = normalize_final_decision(&input.final_decision)?;
    let target_stage = if final_decision == "HIRE" {
        PipelineStage::Offered
    } else {
        PipelineStage::Rejected
    };
    if current_stage_text != target_stage.as_db()
        && !is_valid_transition(&current_stage_text, target_stage.as_db())
    {
        return Err(AppError::InvalidTransition {
            from: current_stage_text.clone(),
            to: target_stage.as_db().to_string(),
        }
        .to_string());
    }

    let inferred_job_id = conn
        .query_row(
            "SELECT linked_job_id FROM candidates WHERE id = ?1",
            [input.candidate_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .flatten();
    let effective_job_id = input.job_id.or(inferred_job_id);

    let latest_evaluation = if let Some(job_id) = effective_job_id {
        conn.query_row(
            r#"
            SELECT id, recommendation
            FROM interview_evaluations
            WHERE candidate_id = ?1 AND job_id = ?2
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            params![input.candidate_id, job_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
    } else {
        conn.query_row(
            r#"
            SELECT id, recommendation
            FROM interview_evaluations
            WHERE candidate_id = ?1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            [input.candidate_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
    };

    let interview_evaluation_id = latest_evaluation.as_ref().map(|value| value.0);
    let ai_recommendation = latest_evaluation.map(|value| value.1);
    let ai_deviation = ai_recommendation
        .as_deref()
        .and_then(map_ai_recommendation_to_final_decision)
        .map(|mapped| mapped != final_decision.as_str())
        .unwrap_or(false);

    let now = now_iso();
    conn.execute(
        r#"
        INSERT INTO hiring_decisions(
            candidate_id, job_id, interview_evaluation_id, ai_recommendation,
            final_decision, reason_code, note, ai_deviation, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            input.candidate_id,
            effective_job_id,
            interview_evaluation_id,
            ai_recommendation.clone(),
            &final_decision,
            &reason_code,
            note.clone(),
            if ai_deviation { 1_i32 } else { 0_i32 },
            now,
            now,
        ],
    )
    .map_err(|error| error.to_string())?;
    let decision_id = conn.last_insert_rowid();

    if current_stage_text != target_stage.as_db() {
        conn.execute(
            "UPDATE candidates SET stage = ?1, updated_at = ?2 WHERE id = ?3",
            params![target_stage.as_db(), now_iso(), input.candidate_id],
        )
        .map_err(|error| error.to_string())?;

        if let Some(job_id) = effective_job_id {
            conn.execute(
                r#"
                INSERT INTO applications(job_id, candidate_id, stage, notes, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(job_id, candidate_id)
                DO UPDATE SET stage = excluded.stage, notes = excluded.notes, updated_at = excluded.updated_at
                "#,
                params![
                    job_id,
                    input.candidate_id,
                    target_stage.as_db(),
                    note.clone(),
                    now,
                    now,
                ],
            )
            .map_err(|error| error.to_string())?;
        }

        let stage_note = note
            .as_deref()
            .map(|value| {
                format!("final_decision={final_decision}; reason_code={reason_code}; note={value}")
            })
            .unwrap_or_else(|| {
                format!("final_decision={final_decision}; reason_code={reason_code}")
            });

        conn.execute(
            r#"
            INSERT INTO pipeline_events(candidate_id, job_id, from_stage, to_stage, note, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                input.candidate_id,
                effective_job_id,
                current_stage_text,
                target_stage.as_db(),
                stage_note,
                now,
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    let record = conn
        .query_row(
            r#"
            SELECT id, candidate_id, job_id, interview_evaluation_id, ai_recommendation,
                   final_decision, reason_code, note, ai_deviation, created_at, updated_at
            FROM hiring_decisions
            WHERE id = ?1
            "#,
            [decision_id],
            |row| {
                let ai_deviation_flag = row.get::<_, i32>(8)? != 0;
                Ok(HiringDecisionRecord {
                    id: row.get(0)?,
                    candidate_id: row.get(1)?,
                    job_id: row.get(2)?,
                    interview_evaluation_id: row.get(3)?,
                    ai_recommendation: row.get(4)?,
                    final_decision: row.get(5)?,
                    reason_code: row.get(6)?,
                    note: row.get(7)?,
                    ai_deviation: ai_deviation_flag,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            },
        )
        .map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "hiring.decision.finalize",
        "hiring_decision",
        Some(record.id.to_string()),
        serde_json::json!({
            "candidateId": record.candidate_id,
            "jobId": record.job_id,
            "finalDecision": record.final_decision,
            "reasonCode": record.reason_code,
            "aiRecommendation": record.ai_recommendation,
            "aiDeviation": record.ai_deviation,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(record)
}

#[tauri::command]
pub(crate) fn list_hiring_decisions(
    state: State<'_, AppState>,
    candidate_id: i64,
) -> Result<Vec<HiringDecisionRecord>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, candidate_id, job_id, interview_evaluation_id, ai_recommendation,
                   final_decision, reason_code, note, ai_deviation, created_at, updated_at
            FROM hiring_decisions
            WHERE candidate_id = ?1
            ORDER BY created_at DESC
            "#,
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([candidate_id], |row| {
            Ok(HiringDecisionRecord {
                id: row.get(0)?,
                candidate_id: row.get(1)?,
                job_id: row.get(2)?,
                interview_evaluation_id: row.get(3)?,
                ai_recommendation: row.get(4)?,
                final_decision: row.get(5)?,
                reason_code: row.get(6)?,
                note: row.get(7)?,
                ai_deviation: row.get::<_, i32>(8)? != 0,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}
