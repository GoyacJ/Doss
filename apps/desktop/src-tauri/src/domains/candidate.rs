use base64::prelude::*;
use rusqlite::{params, params_from_iter, types::Value as SqlValue, Connection, OptionalExtension};
#[cfg(test)]
use serde::Serialize;
use serde_json::Value;
use tauri::State;
use tauri::{AppHandle, Emitter};

use crate::core::cipher::FieldCipher;
use crate::core::error::AppError;
use crate::core::pii::{hash_value, mask_email, mask_phone, normalize_phone};
use crate::core::state::AppState;
use crate::core::time::now_iso;
#[cfg(test)]
use crate::domains::ai_runtime::{
    invoke_cloud_provider, planned_resume_input_mode, read_resume_attachment, resolve_ai_settings,
    AiInvokeProgressEvent,
};
#[cfg(test)]
use crate::domains::recruiting_utils::clamp_score;
#[cfg(test)]
use crate::domains::resume_materializer::ensure_resume_materialized;
#[cfg(test)]
use crate::domains::resume_parser::{
    expected_salary_k_from_parsed_json, parse_skills_from_parsed_json,
};
use crate::domains::resume_parser::{
    extract_resume_content_from_bytes, extract_resume_profile_fields,
    extract_resume_text_from_bytes, parse_resume_text_v2, resume_parser_v3_enabled,
    ResumeTextExtraction,
};
use crate::domains::scoring::run_candidate_ai_analysis_silent;
use crate::domains::sidecar_runtime::try_crawl_resume_for_pending_sync;
use crate::infra::audit::write_audit;
use crate::infra::db::open_connection;
use crate::infra::search_index::sync_candidate_search;
#[cfg(test)]
use crate::models::ai::{AiAnalysisPayload, AiPromptContext, RunAnalysisInput};
use crate::models::ai::{AnalysisRecord, DimensionScore, EvidenceItem};
use crate::models::candidate::{
    Candidate, CandidateListQuery, DecisionListQuery, InterviewListQuery,
    MergeCandidateImportInput, MoveStageInput, NewCandidateInput, PendingCandidate,
    PendingCandidateListQuery, PendingSyncItemResult, PendingSyncMode,
    PendingSyncProgressEventPayload, PendingSyncRunInput, PendingSyncRunResult, PipelineEvent,
    PreviewResumeProfileInput, ResumeProfilePreview, ResumeRecord, SetCandidateQualificationInput,
    SortRule, SyncPendingCandidateInput, UpdateCandidateInput, UpsertPendingCandidatesInput,
    UpsertResumeInput,
};
use crate::models::common::{
    is_valid_transition, resolve_qualification_stage, PageResult, PipelineStage, SourceType,
};
use crate::models::resume::ResumeParsedV2;
use crate::models::scoring::RunCandidateScoringInput;

#[cfg(test)]
const ANALYSIS_PROGRESS_EVENT: &str = "candidate-analysis-progress";
const PENDING_SYNC_PROGRESS_EVENT: &str = "pending-ai-sync-progress";

#[cfg(test)]
#[derive(Debug, Clone)]
struct AnalysisProgressUpdate {
    phase: &'static str,
    status: &'static str,
    kind: &'static str,
    message: String,
    meta: Option<Value>,
}

#[cfg(test)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CandidateAnalysisProgressPayload {
    run_id: String,
    candidate_id: i64,
    phase: String,
    status: String,
    kind: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta: Option<Value>,
    at: String,
}

#[cfg(test)]
fn analysis_progress_update(
    phase: &'static str,
    status: &'static str,
    kind: &'static str,
    message: impl Into<String>,
    meta: Option<Value>,
) -> AnalysisProgressUpdate {
    AnalysisProgressUpdate {
        phase,
        status,
        kind,
        message: message.into(),
        meta,
    }
}

#[cfg(test)]
fn to_analysis_progress_payload(
    run_id: &str,
    candidate_id: i64,
    update: AnalysisProgressUpdate,
) -> CandidateAnalysisProgressPayload {
    CandidateAnalysisProgressPayload {
        run_id: run_id.to_string(),
        candidate_id,
        phase: update.phase.to_string(),
        status: update.status.to_string(),
        kind: update.kind.to_string(),
        message: update.message,
        meta: update.meta,
        at: now_iso(),
    }
}

#[cfg(test)]
fn emit_analysis_progress(
    app_handle: &AppHandle,
    run_id: &str,
    candidate_id: i64,
    update: AnalysisProgressUpdate,
) {
    let payload = to_analysis_progress_payload(run_id, candidate_id, update);
    let _ = app_handle.emit(ANALYSIS_PROGRESS_EVENT, payload);
}

fn emit_pending_sync_progress(app_handle: &AppHandle, payload: PendingSyncProgressEventPayload) {
    let _ = app_handle.emit(PENDING_SYNC_PROGRESS_EVENT, payload);
}

pub(crate) fn merge_candidate_tags(existing: &[String], incoming: &[String]) -> Vec<String> {
    let mut dedup = std::collections::BTreeMap::<String, String>::new();

    for tag in existing.iter().chain(incoming.iter()) {
        let normalized = tag.trim();
        if normalized.is_empty() {
            continue;
        }
        let key = normalized.to_lowercase();
        dedup.entry(key).or_insert_with(|| normalized.to_string());
    }

    dedup
        .values()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
}

pub(crate) fn candidate_from_row(
    row: &rusqlite::Row<'_>,
    cipher: &FieldCipher,
) -> Result<Candidate, rusqlite::Error> {
    let stage_text: String = row.get("stage")?;
    let stage = PipelineStage::from_db(&stage_text).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
    })?;

    let tags_json: String = row.get("tags_json")?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

    let phone_masked = row
        .get::<_, Option<String>>("phone_enc")?
        .and_then(|value| cipher.decrypt(&value).ok())
        .map(|value| mask_phone(&value));

    let email_masked = row
        .get::<_, Option<String>>("email_enc")?
        .and_then(|value| cipher.decrypt(&value).ok())
        .map(|value| mask_email(&value));

    Ok(Candidate {
        id: row.get("id")?,
        external_id: row.get("external_id")?,
        source: row.get("source")?,
        name: row.get("name")?,
        current_company: row.get("current_company")?,
        job_id: row.get("linked_job_id")?,
        job_title: row.get("linked_job_title")?,
        score: row.get("score")?,
        age: row.get("age")?,
        gender: row.get("gender")?,
        years_of_experience: row.get("years_of_experience")?,
        address: row.get("address")?,
        stage,
        tags,
        phone_masked,
        email_masked,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn read_candidate_by_id(
    conn: &Connection,
    candidate_id: i64,
    cipher: &FieldCipher,
) -> Result<Candidate, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, external_id, source, name, current_company, linked_job_id, linked_job_title, score, age, gender, years_of_experience, address, stage, tags_json, phone_enc, email_enc, created_at, updated_at FROM candidates WHERE id = ?1",
        )
        .map_err(|error| error.to_string())?;

    stmt.query_row([candidate_id], |row| candidate_from_row(row, cipher))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn create_candidate(
    state: State<'_, AppState>,
    input: NewCandidateInput,
) -> Result<Candidate, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let now = now_iso();

    let phone_normalized = input.phone.as_deref().map(normalize_phone);
    let phone_hash = phone_normalized.as_deref().map(hash_value);
    let phone_encrypted = phone_normalized
        .as_deref()
        .map(|value| state.cipher.encrypt(value))
        .transpose()
        .map_err(|error| error.to_string())?;

    let email_hash = input.email.as_deref().map(hash_value);
    let email_encrypted = input
        .email
        .as_deref()
        .map(|value| state.cipher.encrypt(value))
        .transpose()
        .map_err(|error| error.to_string())?;

    let tags_json = serde_json::to_string(&input.tags).map_err(|error| error.to_string())?;
    let score = input.score.map(|value| value.clamp(0.0, 100.0));
    let age = input.age.filter(|value| *value >= 0);
    let gender = input
        .gender
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let address = input
        .address
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let (linked_job_id, linked_job_title) = if let Some(job_id) = input.job_id {
        let job_title = conn
            .query_row("SELECT title FROM jobs WHERE id = ?1", [job_id], |row| {
                row.get::<_, String>(0)
            })
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Job {} not found", job_id))?;
        (Some(job_id), Some(job_title))
    } else {
        (None, None)
    };

    conn.execute(
        r#"
        INSERT INTO candidates(
            external_id, source, name, current_company, linked_job_id, linked_job_title, score, age, gender, years_of_experience, address, stage,
            phone_enc, phone_hash, email_enc, email_hash, tags_json, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'NEW', ?12, ?13, ?14, ?15, ?16, ?17, ?18)
        "#,
        params![
            input.external_id,
            input.source.unwrap_or(SourceType::Manual).as_db(),
            input.name,
            input.current_company,
            linked_job_id,
            linked_job_title,
            score,
            age,
            gender,
            input.years_of_experience,
            address,
            phone_encrypted,
            phone_hash,
            email_encrypted,
            email_hash,
            tags_json,
            now,
            now,
        ],
    )
    .map_err(|error| error.to_string())?;

    let candidate_id = conn.last_insert_rowid();

    if let Some(job_id) = input.job_id {
        conn.execute(
            r#"
            INSERT INTO applications(job_id, candidate_id, stage, notes, created_at, updated_at)
            VALUES (?1, ?2, 'NEW', NULL, ?3, ?4)
            ON CONFLICT(job_id, candidate_id)
            DO UPDATE SET updated_at = excluded.updated_at
            "#,
            params![job_id, candidate_id, now, now],
        )
        .map_err(|error| error.to_string())?;
    }

    sync_candidate_search(&conn, candidate_id).map_err(|error| error.to_string())?;

    let candidate = read_candidate_by_id(&conn, candidate_id, &state.cipher)?;

    write_audit(
        &conn,
        "candidate.create",
        "candidate",
        Some(candidate.id.to_string()),
        serde_json::json!({"source": candidate.source, "tags": candidate.tags}),
    )
    .map_err(|error| error.to_string())?;

    Ok(candidate)
}

#[tauri::command]
pub(crate) fn update_candidate(
    state: State<'_, AppState>,
    input: UpdateCandidateInput,
) -> Result<Candidate, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let name = input.name.trim();
    if name.is_empty() {
        return Err("candidate_name_required".to_string());
    }

    let current_company = input
        .current_company
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let normalized_phone = input
        .phone
        .as_deref()
        .map(normalize_phone)
        .filter(|value| !value.is_empty());
    let phone_hash = normalized_phone.as_deref().map(hash_value);
    let phone_enc = normalized_phone
        .as_deref()
        .map(|value| state.cipher.encrypt(value))
        .transpose()
        .map_err(|error| error.to_string())?;

    let normalized_email = input
        .email
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let email_hash = normalized_email.as_deref().map(hash_value);
    let email_enc = normalized_email
        .as_deref()
        .map(|value| state.cipher.encrypt(value))
        .transpose()
        .map_err(|error| error.to_string())?;

    let tags = merge_candidate_tags(&[], &input.tags);
    let tags_json = serde_json::to_string(&tags).map_err(|error| error.to_string())?;
    let score = input.score.map(|value| value.clamp(0.0, 100.0));
    let age = input.age.filter(|value| *value >= 0);
    let gender = input
        .gender
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let address = input
        .address
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let (linked_job_id, linked_job_title) = if let Some(job_id) = input.job_id {
        let job_title = conn
            .query_row("SELECT title FROM jobs WHERE id = ?1", [job_id], |row| {
                row.get::<_, String>(0)
            })
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Job {} not found", job_id))?;
        (Some(job_id), Some(job_title))
    } else {
        (None, None)
    };
    let now = now_iso();
    let affected = conn
        .execute(
            r#"
            UPDATE candidates
            SET
                name = ?1,
                current_company = ?2,
                years_of_experience = ?3,
                score = COALESCE(?4, score),
                age = COALESCE(?5, age),
                gender = COALESCE(?6, gender),
                address = COALESCE(?7, address),
                tags_json = ?8,
                phone_enc = CASE WHEN ?9 IS NOT NULL THEN ?9 ELSE phone_enc END,
                phone_hash = CASE WHEN ?10 IS NOT NULL THEN ?10 ELSE phone_hash END,
                email_enc = CASE WHEN ?11 IS NOT NULL THEN ?11 ELSE email_enc END,
                email_hash = CASE WHEN ?12 IS NOT NULL THEN ?12 ELSE email_hash END,
                linked_job_id = ?13,
                linked_job_title = ?14,
                updated_at = ?15
            WHERE id = ?16
            "#,
            params![
                name,
                current_company,
                input.years_of_experience.max(0.0),
                score,
                age,
                gender,
                address,
                tags_json,
                phone_enc,
                phone_hash,
                email_enc,
                email_hash,
                linked_job_id,
                linked_job_title,
                now,
                input.candidate_id,
            ],
        )
        .map_err(|error| error.to_string())?;

    if affected == 0 {
        return Err(format!("Candidate {} not found", input.candidate_id));
    }

    if let Some(job_id) = linked_job_id {
        let stage_text: String = conn
            .query_row(
                "SELECT stage FROM candidates WHERE id = ?1",
                [input.candidate_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        conn.execute(
            r#"
            INSERT INTO applications(job_id, candidate_id, stage, notes, created_at, updated_at)
            VALUES (?1, ?2, ?3, NULL, ?4, ?5)
            ON CONFLICT(job_id, candidate_id)
            DO UPDATE SET stage = excluded.stage, updated_at = excluded.updated_at
            "#,
            params![job_id, input.candidate_id, stage_text, now, now],
        )
        .map_err(|error| error.to_string())?;
    }

    sync_candidate_search(&conn, input.candidate_id).map_err(|error| error.to_string())?;
    let candidate = read_candidate_by_id(&conn, input.candidate_id, &state.cipher)?;

    write_audit(
        &conn,
        "candidate.update",
        "candidate",
        Some(candidate.id.to_string()),
        serde_json::json!({
            "updatedTagCount": candidate.tags.len(),
            "updatedPhone": normalized_phone.is_some(),
            "updatedEmail": normalized_email.is_some()
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(candidate)
}

#[tauri::command]
pub(crate) fn delete_candidate(
    state: State<'_, AppState>,
    candidate_id: i64,
) -> Result<bool, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    conn.execute(
        "DELETE FROM candidate_search WHERE candidate_id = ?1",
        [candidate_id],
    )
    .map_err(|error| error.to_string())?;

    let affected = conn
        .execute("DELETE FROM candidates WHERE id = ?1", [candidate_id])
        .map_err(|error| error.to_string())?;
    if affected == 0 {
        return Err(format!("Candidate {} not found", candidate_id));
    }

    write_audit(
        &conn,
        "candidate.delete",
        "candidate",
        Some(candidate_id.to_string()),
        serde_json::json!({ "deleted": true }),
    )
    .map_err(|error| error.to_string())?;

    Ok(true)
}

#[tauri::command]
pub(crate) fn set_candidate_qualification(
    state: State<'_, AppState>,
    input: SetCandidateQualificationInput,
) -> Result<Candidate, String> {
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

    let target_stage = resolve_qualification_stage(&current_stage_text, input.qualified);
    if let Some(next_stage) = target_stage {
        let now = now_iso();
        conn.execute(
            "UPDATE candidates SET stage = ?1, updated_at = ?2 WHERE id = ?3",
            params![next_stage, now, input.candidate_id],
        )
        .map_err(|error| error.to_string())?;

        conn.execute(
            "UPDATE applications SET stage = ?1, updated_at = ?2 WHERE candidate_id = ?3",
            params![next_stage, now, input.candidate_id],
        )
        .map_err(|error| error.to_string())?;

        let note = input.note.clone().or_else(|| {
            if input.qualified {
                Some("已启用候选资格".to_string())
            } else {
                Some("已取消候选资格".to_string())
            }
        });

        conn.execute(
            r#"
            INSERT INTO pipeline_events(candidate_id, job_id, from_stage, to_stage, note, created_at)
            VALUES (?1, NULL, ?2, ?3, ?4, ?5)
            "#,
            params![input.candidate_id, current_stage_text, next_stage, note, now],
        )
        .map_err(|error| error.to_string())?;
    }

    let candidate = read_candidate_by_id(&conn, input.candidate_id, &state.cipher)?;
    write_audit(
        &conn,
        "candidate.qualification.update",
        "candidate",
        Some(candidate.id.to_string()),
        serde_json::json!({
            "qualified": input.qualified,
            "stageChanged": target_stage.is_some()
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(candidate)
}

#[tauri::command]
pub(crate) fn merge_candidate_import(
    state: State<'_, AppState>,
    input: MergeCandidateImportInput,
) -> Result<Candidate, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;

    let existing = conn
        .query_row(
            "SELECT tags_json FROM candidates WHERE id = ?1",
            [input.candidate_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;

    let existing_tags_json =
        existing.ok_or_else(|| format!("Candidate {} not found", input.candidate_id))?;
    let existing_tags: Vec<String> = serde_json::from_str(&existing_tags_json).unwrap_or_default();
    let incoming_tags = input.tags.unwrap_or_default();
    let merged_tags = merge_candidate_tags(&existing_tags, &incoming_tags);
    let merged_tags_json =
        serde_json::to_string(&merged_tags).map_err(|error| error.to_string())?;

    let incoming_company = input
        .current_company
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let incoming_address = input
        .address
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let incoming_years = input.years_of_experience.map(|value| value.max(0.0));

    let normalized_phone = input
        .phone
        .as_deref()
        .map(normalize_phone)
        .filter(|value| !value.is_empty());
    let phone_hash = normalized_phone.as_deref().map(hash_value);
    let phone_enc = normalized_phone
        .as_deref()
        .map(|value| state.cipher.encrypt(value))
        .transpose()
        .map_err(|error| error.to_string())?;

    let normalized_email = input
        .email
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let email_hash = normalized_email.as_deref().map(hash_value);
    let email_enc = normalized_email
        .as_deref()
        .map(|value| state.cipher.encrypt(value))
        .transpose()
        .map_err(|error| error.to_string())?;

    let now = now_iso();
    let updated = conn
        .execute(
            r#"
            UPDATE candidates
            SET
                current_company = CASE
                    WHEN (current_company IS NULL OR trim(current_company) = '') AND ?1 IS NOT NULL
                    THEN ?1
                    ELSE current_company
                END,
                years_of_experience = CASE
                    WHEN ?2 IS NOT NULL AND ?2 > years_of_experience
                    THEN ?2
                    ELSE years_of_experience
                END,
                address = CASE
                    WHEN (address IS NULL OR trim(address) = '') AND ?3 IS NOT NULL
                    THEN ?3
                    ELSE address
                END,
                tags_json = ?4,
                phone_enc = COALESCE(phone_enc, ?5),
                phone_hash = COALESCE(phone_hash, ?6),
                email_enc = COALESCE(email_enc, ?7),
                email_hash = COALESCE(email_hash, ?8),
                updated_at = ?9
            WHERE id = ?10
            "#,
            params![
                incoming_company,
                incoming_years,
                incoming_address,
                merged_tags_json,
                phone_enc,
                phone_hash,
                email_enc,
                email_hash,
                now,
                input.candidate_id,
            ],
        )
        .map_err(|error| error.to_string())?;

    if updated == 0 {
        return Err(format!("Candidate {} not found", input.candidate_id));
    }

    if let Some(job_id) = input.job_id {
        let stage_text: String = conn
            .query_row(
                "SELECT stage FROM candidates WHERE id = ?1",
                [input.candidate_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;

        conn.execute(
            r#"
            INSERT INTO applications(job_id, candidate_id, stage, notes, created_at, updated_at)
            VALUES (?1, ?2, ?3, NULL, ?4, ?5)
            ON CONFLICT(job_id, candidate_id)
            DO UPDATE SET stage = excluded.stage, updated_at = excluded.updated_at
            "#,
            params![job_id, input.candidate_id, stage_text, now, now],
        )
        .map_err(|error| error.to_string())?;

        let linked_job_title = conn
            .query_row("SELECT title FROM jobs WHERE id = ?1", [job_id], |row| {
                row.get::<_, String>(0)
            })
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Job {} not found", job_id))?;
        conn.execute(
            "UPDATE candidates SET linked_job_id = ?1, linked_job_title = ?2, updated_at = ?3 WHERE id = ?4",
            params![job_id, linked_job_title, now, input.candidate_id],
        )
        .map_err(|error| error.to_string())?;
    }

    sync_candidate_search(&conn, input.candidate_id).map_err(|error| error.to_string())?;

    let candidate = read_candidate_by_id(&conn, input.candidate_id, &state.cipher)?;

    write_audit(
        &conn,
        "candidate.merge",
        "candidate",
        Some(candidate.id.to_string()),
        serde_json::json!({
            "jobId": input.job_id,
            "mergedTagCount": candidate.tags.len(),
            "hadPhoneInput": normalized_phone.is_some(),
            "hadEmailInput": normalized_email.is_some()
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(candidate)
}

#[tauri::command]
pub(crate) fn list_candidates(
    state: State<'_, AppState>,
    stage: Option<PipelineStage>,
) -> Result<Vec<Candidate>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    if let Some(filter_stage) = stage {
        let mut stmt = conn
            .prepare(
                "SELECT id, external_id, source, name, current_company, linked_job_id, linked_job_title, score, age, gender, years_of_experience, address, stage, tags_json, phone_enc, email_enc, created_at, updated_at FROM candidates WHERE stage = ?1 ORDER BY updated_at DESC",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map([filter_stage.as_db()], |row| {
                candidate_from_row(row, &state.cipher)
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    } else {
        let mut stmt = conn
            .prepare(
                "SELECT id, external_id, source, name, current_company, linked_job_id, linked_job_title, score, age, gender, years_of_experience, address, stage, tags_json, phone_enc, email_enc, created_at, updated_at FROM candidates ORDER BY updated_at DESC",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map([], |row| candidate_from_row(row, &state.cipher))
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }
}

fn read_candidates_page(
    conn: &Connection,
    cipher: &FieldCipher,
    where_clauses: &[String],
    params: &[SqlValue],
    order_by: &str,
    page: i64,
    page_size: i64,
) -> Result<PageResult<Candidate>, String> {
    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_clauses.join(" AND "))
    };

    let count_sql = format!("SELECT COUNT(1) FROM candidates{where_sql}");
    let total: i64 = conn
        .query_row(&count_sql, params_from_iter(params.iter()), |row| {
            row.get(0)
        })
        .map_err(|error| error.to_string())?;

    let mut query_params = params.to_vec();
    query_params.push(SqlValue::Integer(page_size));
    query_params.push(SqlValue::Integer((page - 1) * page_size));

    let list_sql = format!(
        "SELECT id, external_id, source, name, current_company, linked_job_id, linked_job_title, score, age, gender, years_of_experience, address, stage, tags_json, phone_enc, email_enc, created_at, updated_at FROM candidates{where_sql} ORDER BY {order_by} LIMIT ? OFFSET ?"
    );
    let mut stmt = conn.prepare(&list_sql).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params_from_iter(query_params.iter()), |row| {
            candidate_from_row(row, cipher)
        })
        .map_err(|error| error.to_string())?;
    let items = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(PageResult {
        items,
        page,
        page_size,
        total,
    })
}

fn normalize_sort_direction(direction: &str) -> &'static str {
    if direction.trim().eq_ignore_ascii_case("asc") {
        return "ASC";
    }
    "DESC"
}

pub(crate) fn build_order_by_from_rules(
    sorts: Option<&Vec<SortRule>>,
    allowed_columns: &[(&str, &str)],
    default_order_by: &str,
) -> String {
    let Some(sort_rules) = sorts else {
        return default_order_by.to_string();
    };

    let mut order_parts = Vec::<String>::new();
    let mut seen_fields = std::collections::BTreeSet::<String>::new();

    for rule in sort_rules.iter().take(3) {
        let field_key = rule.field.trim().to_ascii_lowercase();
        if field_key.is_empty() || seen_fields.contains(&field_key) {
            continue;
        }

        let Some((_, column)) = allowed_columns
            .iter()
            .find(|(field, _)| *field == field_key)
        else {
            continue;
        };

        seen_fields.insert(field_key);
        let direction = normalize_sort_direction(&rule.direction);
        order_parts.push(format!("{column} IS NULL ASC"));
        order_parts.push(format!("{column} {direction}"));
    }

    if order_parts.is_empty() {
        return default_order_by.to_string();
    }

    order_parts.push("id DESC".to_string());
    order_parts.join(", ")
}

#[tauri::command]
pub(crate) fn list_candidates_page(
    state: State<'_, AppState>,
    input: Option<CandidateListQuery>,
) -> Result<PageResult<Candidate>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let query = input.unwrap_or(CandidateListQuery {
        page: crate::models::common::PageQuery {
            page: None,
            page_size: None,
        },
        job_id: None,
        name_like: None,
        stage: None,
        sorts: None,
    });

    let mut where_clauses = Vec::<String>::new();
    let mut params = Vec::<SqlValue>::new();

    if let Some(job_id) = query.job_id {
        where_clauses.push("linked_job_id = ?".to_string());
        params.push(SqlValue::Integer(job_id));
    }
    if let Some(stage) = query.stage {
        where_clauses.push("stage = ?".to_string());
        params.push(SqlValue::Text(stage.as_db().to_string()));
    }
    if let Some(name_like) = query
        .name_like
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        where_clauses.push("name LIKE ?".to_string());
        params.push(SqlValue::Text(format!("%{name_like}%")));
    }

    let candidate_sort_columns = [
        ("name", "name"),
        ("current_company", "current_company"),
        ("job_title", "linked_job_title"),
        ("score", "score"),
        ("stage", "stage"),
        ("years_of_experience", "years_of_experience"),
        ("updated_at", "updated_at"),
        ("created_at", "created_at"),
    ];
    let order_by = build_order_by_from_rules(
        query.sorts.as_ref(),
        &candidate_sort_columns,
        "linked_job_title IS NULL ASC, linked_job_title ASC, score IS NULL ASC, score DESC, updated_at DESC, id DESC",
    );

    read_candidates_page(
        &conn,
        &state.cipher,
        &where_clauses,
        &params,
        &order_by,
        query.page.normalized_page(),
        query.page.normalized_page_size(),
    )
}

#[tauri::command]
pub(crate) fn list_interview_candidates_page(
    state: State<'_, AppState>,
    input: Option<InterviewListQuery>,
) -> Result<PageResult<Candidate>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let query = input.unwrap_or(InterviewListQuery {
        page: crate::models::common::PageQuery {
            page: None,
            page_size: None,
        },
        job_id: None,
        name_like: None,
        sorts: None,
    });

    let mut where_clauses = vec!["stage = 'INTERVIEW'".to_string()];
    let mut params = Vec::<SqlValue>::new();
    if let Some(job_id) = query.job_id {
        where_clauses.push("linked_job_id = ?".to_string());
        params.push(SqlValue::Integer(job_id));
    }
    if let Some(name_like) = query
        .name_like
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        where_clauses.push("name LIKE ?".to_string());
        params.push(SqlValue::Text(format!("%{name_like}%")));
    }

    let interview_sort_columns = [
        ("name", "name"),
        ("job_title", "linked_job_title"),
        ("stage", "stage"),
        ("updated_at", "updated_at"),
        ("created_at", "created_at"),
    ];
    let order_by = build_order_by_from_rules(
        query.sorts.as_ref(),
        &interview_sort_columns,
        "updated_at DESC, id DESC",
    );

    read_candidates_page(
        &conn,
        &state.cipher,
        &where_clauses,
        &params,
        &order_by,
        query.page.normalized_page(),
        query.page.normalized_page_size(),
    )
}

#[tauri::command]
pub(crate) fn list_decision_candidates_page(
    state: State<'_, AppState>,
    input: Option<DecisionListQuery>,
) -> Result<PageResult<Candidate>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let query = input.unwrap_or(DecisionListQuery {
        page: crate::models::common::PageQuery {
            page: None,
            page_size: None,
        },
        job_id: None,
        name_like: None,
        interview_passed: None,
        sorts: None,
    });

    let mut where_clauses = vec![
        "(EXISTS (SELECT 1 FROM interview_evaluations ie WHERE ie.candidate_id = candidates.id) OR EXISTS (SELECT 1 FROM hiring_decisions hd WHERE hd.candidate_id = candidates.id))"
            .to_string(),
    ];
    let mut params = Vec::<SqlValue>::new();
    if let Some(job_id) = query.job_id {
        where_clauses.push("linked_job_id = ?".to_string());
        params.push(SqlValue::Integer(job_id));
    }
    if let Some(name_like) = query
        .name_like
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        where_clauses.push("name LIKE ?".to_string());
        params.push(SqlValue::Text(format!("%{name_like}%")));
    }
    if let Some(interview_passed) = query.interview_passed {
        if interview_passed {
            where_clauses.push("EXISTS (SELECT 1 FROM interview_evaluations ie2 WHERE ie2.candidate_id = candidates.id AND ie2.recommendation = 'HIRE')".to_string());
        } else {
            where_clauses.push("EXISTS (SELECT 1 FROM interview_evaluations ie2 WHERE ie2.candidate_id = candidates.id) AND NOT EXISTS (SELECT 1 FROM interview_evaluations ie3 WHERE ie3.candidate_id = candidates.id AND ie3.recommendation = 'HIRE')".to_string());
        }
    }

    let decision_sort_columns = [
        ("name", "name"),
        ("job_title", "linked_job_title"),
        ("stage", "stage"),
        ("updated_at", "updated_at"),
        ("created_at", "created_at"),
    ];
    let order_by = build_order_by_from_rules(
        query.sorts.as_ref(),
        &decision_sort_columns,
        "updated_at DESC, id DESC",
    );

    read_candidates_page(
        &conn,
        &state.cipher,
        &where_clauses,
        &params,
        &order_by,
        query.page.normalized_page(),
        query.page.normalized_page_size(),
    )
}

fn build_pending_dedupe_key(name: &str, age: Option<i32>, address: Option<&str>) -> String {
    let normalized_name = name.trim().to_lowercase();
    let normalized_age = age.map(|value| value.to_string()).unwrap_or_default();
    let normalized_address = address.unwrap_or("").trim().to_lowercase();
    format!("{normalized_name}|{normalized_age}|{normalized_address}")
}

fn pending_candidate_from_row(
    row: &rusqlite::Row<'_>,
    cipher: &FieldCipher,
) -> Result<PendingCandidate, rusqlite::Error> {
    let tags_text: String = row.get("tags_json")?;
    let resume_parsed_text: String = row.get("resume_parsed_json")?;
    let phone_masked = row
        .get::<_, Option<String>>("phone_enc")?
        .and_then(|value| cipher.decrypt(&value).ok())
        .map(|value| mask_phone(&value));
    let email_masked = row
        .get::<_, Option<String>>("email_enc")?
        .and_then(|value| cipher.decrypt(&value).ok())
        .map(|value| mask_email(&value));

    Ok(PendingCandidate {
        id: row.get("id")?,
        source: row.get("source")?,
        external_id: row.get("external_id")?,
        name: row.get("name")?,
        current_company: row.get("current_company")?,
        job_id: row.get("linked_job_id")?,
        job_title: row.get("linked_job_title")?,
        age: row.get("age")?,
        gender: row.get("gender")?,
        years_of_experience: row.get("years_of_experience")?,
        tags: serde_json::from_str(&tags_text).unwrap_or_default(),
        phone_masked,
        email_masked,
        address: row.get("address")?,
        extra_notes: row.get("extra_notes")?,
        resume_raw_text: row.get("resume_raw_text")?,
        resume_parsed: serde_json::from_str(&resume_parsed_text)
            .unwrap_or(Value::Object(Default::default())),
        dedupe_key: row.get("dedupe_key")?,
        sync_status: row.get("sync_status")?,
        sync_error_code: row.get("sync_error_code")?,
        sync_error_message: row.get("sync_error_message")?,
        candidate_id: row.get("candidate_id")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

#[tauri::command]
pub(crate) fn upsert_pending_candidates(
    state: State<'_, AppState>,
    input: UpsertPendingCandidatesInput,
) -> Result<Vec<PendingCandidate>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let now = now_iso();
    let mut upserted = Vec::<PendingCandidate>::new();

    for item in input.items {
        let name = item.name.trim().to_string();
        if name.is_empty() {
            continue;
        }

        let source = item
            .source
            .unwrap_or(SourceType::Manual)
            .as_db()
            .to_string();
        let years = item.years_of_experience.unwrap_or(0.0).max(0.0);
        let tags = merge_candidate_tags(&[], &item.tags.unwrap_or_default());
        let tags_json = serde_json::to_string(&tags).map_err(|error| error.to_string())?;
        let resume_parsed = item
            .resume_parsed
            .unwrap_or(Value::Object(Default::default()));
        let dedupe_key = item
            .dedupe_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                build_pending_dedupe_key(
                    &name,
                    item.age.filter(|value| *value >= 0),
                    item.address.as_deref(),
                )
            });

        let normalized_phone = item
            .phone
            .as_deref()
            .map(normalize_phone)
            .filter(|value| !value.is_empty());
        let phone_hash = normalized_phone.as_deref().map(hash_value);
        let phone_enc = normalized_phone
            .as_deref()
            .map(|value| state.cipher.encrypt(value))
            .transpose()
            .map_err(|error| error.to_string())?;

        let normalized_email = item
            .email
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let email_hash = normalized_email.as_deref().map(hash_value);
        let email_enc = normalized_email
            .as_deref()
            .map(|value| state.cipher.encrypt(value))
            .transpose()
            .map_err(|error| error.to_string())?;

        let (linked_job_id, linked_job_title) = if let Some(job_id) = item.job_id {
            let job_title = conn
                .query_row("SELECT title FROM jobs WHERE id = ?1", [job_id], |row| {
                    row.get::<_, String>(0)
                })
                .optional()
                .map_err(|error| error.to_string())?
                .ok_or_else(|| format!("Job {} not found", job_id))?;
            (Some(job_id), Some(job_title))
        } else {
            (None, None)
        };

        conn.execute(
            r#"
            INSERT INTO pending_candidates(
                source, external_id, name, current_company, linked_job_id, linked_job_title,
                age, gender, years_of_experience, tags_json,
                phone_enc, phone_hash, email_enc, email_hash,
                address, extra_notes, resume_raw_text, resume_parsed_json,
                dedupe_key, sync_status, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, 'UNSYNCED', ?20, ?21)
            ON CONFLICT(dedupe_key)
            DO UPDATE SET
                source = excluded.source,
                external_id = excluded.external_id,
                name = excluded.name,
                current_company = excluded.current_company,
                linked_job_id = excluded.linked_job_id,
                linked_job_title = excluded.linked_job_title,
                age = excluded.age,
                gender = excluded.gender,
                years_of_experience = excluded.years_of_experience,
                tags_json = excluded.tags_json,
                phone_enc = COALESCE(pending_candidates.phone_enc, excluded.phone_enc),
                phone_hash = COALESCE(pending_candidates.phone_hash, excluded.phone_hash),
                email_enc = COALESCE(pending_candidates.email_enc, excluded.email_enc),
                email_hash = COALESCE(pending_candidates.email_hash, excluded.email_hash),
                address = excluded.address,
                extra_notes = excluded.extra_notes,
                resume_raw_text = excluded.resume_raw_text,
                resume_parsed_json = excluded.resume_parsed_json,
                sync_status = CASE
                    WHEN pending_candidates.sync_status = 'SYNCED' THEN pending_candidates.sync_status
                    ELSE 'UNSYNCED'
                END,
                updated_at = excluded.updated_at
            "#,
            params![
                source,
                item.external_id,
                name,
                item.current_company,
                linked_job_id,
                linked_job_title,
                item.age.filter(|value| *value >= 0),
                item.gender,
                years,
                tags_json,
                phone_enc,
                phone_hash,
                email_enc,
                email_hash,
                item.address,
                item.extra_notes,
                item.resume_raw_text,
                resume_parsed.to_string(),
                dedupe_key,
                now,
                now,
            ],
        )
        .map_err(|error| error.to_string())?;

        let pending = conn
            .query_row(
                "SELECT id, source, external_id, name, current_company, linked_job_id, linked_job_title, age, gender, years_of_experience, tags_json, phone_enc, email_enc, address, extra_notes, resume_raw_text, resume_parsed_json, dedupe_key, sync_status, sync_error_code, sync_error_message, candidate_id, created_at, updated_at FROM pending_candidates WHERE dedupe_key = ?1",
                [dedupe_key],
                |row| pending_candidate_from_row(row, &state.cipher),
            )
            .map_err(|error| error.to_string())?;
        upserted.push(pending);
    }

    write_audit(
        &conn,
        "pending_candidates.upsert",
        "pending_candidate",
        None,
        serde_json::json!({ "count": upserted.len() }),
    )
    .map_err(|error| error.to_string())?;

    Ok(upserted)
}

#[tauri::command]
pub(crate) fn list_pending_candidates(
    state: State<'_, AppState>,
    input: Option<PendingCandidateListQuery>,
) -> Result<PageResult<PendingCandidate>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let query = input.unwrap_or(PendingCandidateListQuery {
        page: crate::models::common::PageQuery {
            page: None,
            page_size: None,
        },
        sync_status: None,
        name_like: None,
        job_id: None,
    });

    let mut where_clauses = Vec::<String>::new();
    let mut params = Vec::<SqlValue>::new();
    if let Some(sync_status) = query
        .sync_status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        where_clauses.push("sync_status = ?".to_string());
        params.push(SqlValue::Text(sync_status.to_uppercase()));
    }
    if let Some(name_like) = query
        .name_like
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        where_clauses.push("name LIKE ?".to_string());
        params.push(SqlValue::Text(format!("%{name_like}%")));
    }
    if let Some(job_id) = query.job_id {
        where_clauses.push("linked_job_id = ?".to_string());
        params.push(SqlValue::Integer(job_id));
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_clauses.join(" AND "))
    };
    let total_sql = format!("SELECT COUNT(1) FROM pending_candidates{where_sql}");
    let total: i64 = conn
        .query_row(&total_sql, params_from_iter(params.iter()), |row| {
            row.get(0)
        })
        .map_err(|error| error.to_string())?;

    let page = query.page.normalized_page();
    let page_size = query.page.normalized_page_size();
    let mut query_params = params.clone();
    query_params.push(SqlValue::Integer(page_size));
    query_params.push(SqlValue::Integer(query.page.offset()));

    let list_sql = format!(
        "SELECT id, source, external_id, name, current_company, linked_job_id, linked_job_title, age, gender, years_of_experience, tags_json, phone_enc, email_enc, address, extra_notes, resume_raw_text, resume_parsed_json, dedupe_key, sync_status, sync_error_code, sync_error_message, candidate_id, created_at, updated_at FROM pending_candidates{where_sql} ORDER BY updated_at DESC LIMIT ? OFFSET ?"
    );
    let mut stmt = conn.prepare(&list_sql).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params_from_iter(query_params.iter()), |row| {
            pending_candidate_from_row(row, &state.cipher)
        })
        .map_err(|error| error.to_string())?;
    let items = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(PageResult {
        items,
        page,
        page_size,
        total,
    })
}

#[tauri::command]
pub(crate) fn list_pending_candidates_page(
    state: State<'_, AppState>,
    input: Option<PendingCandidateListQuery>,
) -> Result<PageResult<PendingCandidate>, String> {
    list_pending_candidates(state, input)
}

fn sync_pending_candidate_to_candidate_inner(
    state: &AppState,
    input: SyncPendingCandidateInput,
) -> Result<Candidate, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;

    let pending = conn
        .query_row(
            "SELECT id, source, external_id, name, current_company, linked_job_id, linked_job_title, age, gender, years_of_experience, tags_json, phone_enc, phone_hash, email_enc, email_hash, address, resume_raw_text, resume_parsed_json, candidate_id FROM pending_candidates WHERE id = ?1",
            [input.pending_candidate_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<i32>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, f64>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, Option<String>>(12)?,
                    row.get::<_, Option<String>>(13)?,
                    row.get::<_, Option<String>>(14)?,
                    row.get::<_, Option<String>>(15)?,
                    row.get::<_, Option<String>>(16)?,
                    row.get::<_, String>(17)?,
                    row.get::<_, Option<i64>>(18)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Pending candidate {} not found", input.pending_candidate_id))?;

    let existing_candidate_id = if let Some(candidate_id) = pending.18 {
        conn.query_row(
            "SELECT id FROM candidates WHERE id = ?1",
            [candidate_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
    } else if let Some(email_hash) = pending.14.as_deref() {
        conn.query_row(
            "SELECT id FROM candidates WHERE email_hash = ?1 LIMIT 1",
            [email_hash],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
    } else if let Some(phone_hash) = pending.12.as_deref() {
        conn.query_row(
            "SELECT id FROM candidates WHERE phone_hash = ?1 LIMIT 1",
            [phone_hash],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
    } else {
        None
    };

    let pending_tags: Vec<String> = serde_json::from_str(&pending.10).unwrap_or_default();
    let now = now_iso();
    let candidate_id = if let Some(candidate_id) = existing_candidate_id {
        let existing_tags_json: String = conn
            .query_row(
                "SELECT tags_json FROM candidates WHERE id = ?1",
                [candidate_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        let existing_tags: Vec<String> =
            serde_json::from_str(&existing_tags_json).unwrap_or_default();
        let merged_tags = merge_candidate_tags(&existing_tags, &pending_tags);
        let merged_tags_json =
            serde_json::to_string(&merged_tags).map_err(|error| error.to_string())?;

        conn.execute(
            r#"
            UPDATE candidates
            SET
                external_id = COALESCE(external_id, ?1),
                source = ?2,
                name = ?3,
                current_company = CASE
                    WHEN (current_company IS NULL OR trim(current_company) = '') AND ?4 IS NOT NULL
                    THEN ?4
                    ELSE current_company
                END,
                linked_job_id = COALESCE(linked_job_id, ?5),
                linked_job_title = COALESCE(linked_job_title, ?6),
                age = COALESCE(age, ?7),
                gender = COALESCE(gender, ?8),
                years_of_experience = CASE
                    WHEN ?9 > years_of_experience THEN ?9
                    ELSE years_of_experience
                END,
                address = CASE
                    WHEN (address IS NULL OR trim(address) = '') AND ?10 IS NOT NULL
                    THEN ?10
                    ELSE address
                END,
                tags_json = ?11,
                phone_enc = COALESCE(phone_enc, ?12),
                phone_hash = COALESCE(phone_hash, ?13),
                email_enc = COALESCE(email_enc, ?14),
                email_hash = COALESCE(email_hash, ?15),
                updated_at = ?16
            WHERE id = ?17
            "#,
            params![
                pending.2,
                pending.1,
                pending.3,
                pending.4,
                pending.5,
                pending.6,
                pending.7,
                pending.8,
                pending.9,
                pending.15,
                merged_tags_json,
                pending.11,
                pending.12,
                pending.13,
                pending.14,
                now,
                candidate_id,
            ],
        )
        .map_err(|error| error.to_string())?;

        candidate_id
    } else {
        conn.execute(
            r#"
            INSERT INTO candidates(
                external_id, source, name, current_company, linked_job_id, linked_job_title,
                score, age, gender, years_of_experience, address, stage,
                phone_enc, phone_hash, email_enc, email_hash, tags_json, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, ?8, ?9, ?10, 'SCREENING', ?11, ?12, ?13, ?14, ?15, ?16, ?17)
            "#,
            params![
                pending.2,
                pending.1,
                pending.3,
                pending.4,
                pending.5,
                pending.6,
                pending.7,
                pending.8,
                pending.9,
                pending.15,
                pending.11,
                pending.12,
                pending.13,
                pending.14,
                pending.10,
                now,
                now,
            ],
        )
        .map_err(|error| error.to_string())?;
        conn.last_insert_rowid()
    };

    conn.execute(
        "UPDATE candidates SET stage = 'SCREENING', updated_at = ?1 WHERE id = ?2",
        params![now, candidate_id],
    )
    .map_err(|error| error.to_string())?;

    if let Some(job_id) = pending.5 {
        let stage_text: String = conn
            .query_row(
                "SELECT stage FROM candidates WHERE id = ?1",
                [candidate_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        conn.execute(
            r#"
            INSERT INTO applications(job_id, candidate_id, stage, notes, created_at, updated_at)
            VALUES (?1, ?2, ?3, NULL, ?4, ?5)
            ON CONFLICT(job_id, candidate_id)
            DO UPDATE SET stage = excluded.stage, updated_at = excluded.updated_at
            "#,
            params![job_id, candidate_id, stage_text, now, now],
        )
        .map_err(|error| error.to_string())?;
    }

    let resume_raw_text = pending
        .16
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_default();
    if !resume_raw_text.is_empty() || pending.17 != "{}" {
        conn.execute(
            r#"
            INSERT INTO resumes(candidate_id, source, raw_text, parsed_json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(candidate_id)
            DO UPDATE SET source = excluded.source, raw_text = excluded.raw_text, parsed_json = excluded.parsed_json, updated_at = excluded.updated_at
            "#,
            params![candidate_id, pending.1, resume_raw_text, pending.17, now, now],
        )
        .map_err(|error| error.to_string())?;
    }

    conn.execute(
        r#"
        UPDATE pending_candidates
        SET sync_status = 'SYNCED',
            sync_error_code = NULL,
            sync_error_message = NULL,
            candidate_id = ?1,
            updated_at = ?2
        WHERE id = ?3
        "#,
        params![candidate_id, now_iso(), input.pending_candidate_id],
    )
    .map_err(|error| error.to_string())?;

    sync_candidate_search(&conn, candidate_id).map_err(|error| error.to_string())?;
    let _ = input.run_screening.unwrap_or(false);

    let candidate = read_candidate_by_id(&conn, candidate_id, &state.cipher)?;
    write_audit(
        &conn,
        "pending_candidate.sync",
        "pending_candidate",
        Some(input.pending_candidate_id.to_string()),
        serde_json::json!({
            "pendingCandidateId": input.pending_candidate_id,
            "candidateId": candidate.id
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(candidate)
}

#[tauri::command]
pub(crate) fn sync_pending_candidate_to_candidate(
    state: State<'_, AppState>,
    input: SyncPendingCandidateInput,
) -> Result<Candidate, String> {
    sync_pending_candidate_to_candidate_inner(state.inner(), input)
}

fn pending_ids_for_sync(
    conn: &Connection,
    input: &PendingSyncRunInput,
) -> Result<Vec<i64>, String> {
    match input.mode {
        PendingSyncMode::Single => {
            let id = input
                .pending_candidate_id
                .ok_or_else(|| "pending_candidate_id_required".to_string())?;
            Ok(vec![id])
        }
        PendingSyncMode::Multi => Ok(input
            .pending_candidate_ids
            .clone()
            .unwrap_or_default()
            .into_iter()
            .filter(|item| *item > 0)
            .collect()),
        PendingSyncMode::Filtered => {
            let mut where_clauses = Vec::<String>::new();
            let mut params = Vec::<SqlValue>::new();
            if let Some(filter) = input.filter.as_ref() {
                if let Some(sync_status) = filter
                    .sync_status
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    where_clauses.push("sync_status = ?".to_string());
                    params.push(SqlValue::Text(sync_status.to_uppercase()));
                }
                if let Some(name_like) = filter
                    .name_like
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    where_clauses.push("name LIKE ?".to_string());
                    params.push(SqlValue::Text(format!("%{name_like}%")));
                }
                if let Some(job_id) = filter.job_id {
                    where_clauses.push("linked_job_id = ?".to_string());
                    params.push(SqlValue::Integer(job_id));
                }
            }
            let where_sql = if where_clauses.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", where_clauses.join(" AND "))
            };
            let sql = format!(
                "SELECT id FROM pending_candidates{where_sql} ORDER BY updated_at DESC, id DESC"
            );
            let mut stmt = conn.prepare(&sql).map_err(|error| error.to_string())?;
            let rows = stmt
                .query_map(params_from_iter(params.iter()), |row| row.get::<_, i64>(0))
                .map_err(|error| error.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())
        }
    }
}

fn parse_sidecar_resume_payload(payload: &Value) -> (Option<String>, Option<Value>) {
    let root = payload.get("output").unwrap_or(payload);
    let raw_text = root
        .get("raw_text")
        .or_else(|| root.get("rawText"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let parsed = root
        .get("parsed")
        .or_else(|| root.get("resumeParsed"))
        .filter(|value| value.is_object() || value.is_array())
        .cloned();
    (raw_text, parsed)
}

#[tauri::command]
pub(crate) async fn run_pending_candidates_ai_sync(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    input: PendingSyncRunInput,
) -> Result<PendingSyncRunResult, String> {
    let run_id = input
        .run_id
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("pending-ai-sync-{}", now_iso()));
    let app_state = state.inner().clone();
    let app_handle_for_task = app_handle.clone();
    let run_id_for_task = run_id.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let conn = open_connection(&app_state.db_path).map_err(|error| error.to_string())?;
        let pending_ids = pending_ids_for_sync(&conn, &input)?;
        let total = pending_ids.len() as i64;
        let mut completed = 0_i64;
        let mut success = 0_i64;
        let mut failed = 0_i64;
        let mut outcomes = Vec::<PendingSyncItemResult>::new();

        emit_pending_sync_progress(
            &app_handle_for_task,
            PendingSyncProgressEventPayload {
                run_id: run_id_for_task.clone(),
                total,
                completed,
                success,
                failed,
                current_pending_candidate_id: None,
                current_candidate_id: None,
                current_status: Some("RUNNING".to_string()),
                message: format!("开始 AI 同步，共 {total} 条待定人"),
                at: now_iso(),
            },
        );

        for pending_id in pending_ids {
            let mut current_candidate_id: Option<i64> = None;
            let item_result = (|| -> Result<PendingSyncItemResult, String> {
                let conn =
                    open_connection(&app_state.db_path).map_err(|error| error.to_string())?;
                let pending_row = conn
                    .query_row(
                        "SELECT source, external_id FROM pending_candidates WHERE id = ?1",
                        [pending_id],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("Pending candidate {pending_id} not found"))?;

                if let Some(external_id) = pending_row.1.as_deref() {
                    if let Ok(sidecar_payload) =
                        try_crawl_resume_for_pending_sync(&app_state, &pending_row.0, external_id)
                    {
                        let (raw_text, parsed) = parse_sidecar_resume_payload(&sidecar_payload);
                        if raw_text.is_some() || parsed.is_some() {
                            let now = now_iso();
                            let parsed_json_text = parsed
                                .unwrap_or_else(|| Value::Object(Default::default()))
                                .to_string();
                            conn.execute(
                                r#"
                                UPDATE pending_candidates
                                SET resume_raw_text = COALESCE(?1, resume_raw_text),
                                    resume_parsed_json = CASE
                                        WHEN ?2 <> '{}' THEN ?2
                                        ELSE resume_parsed_json
                                    END,
                                    updated_at = ?3
                                WHERE id = ?4
                                "#,
                                params![raw_text, parsed_json_text, now, pending_id],
                            )
                            .map_err(|error| error.to_string())?;
                        }
                    }
                }

                let candidate = sync_pending_candidate_to_candidate_inner(
                    &app_state,
                    SyncPendingCandidateInput {
                        pending_candidate_id: pending_id,
                        run_screening: Some(true),
                    },
                )?;
                current_candidate_id = Some(candidate.id);

                let _ = run_candidate_ai_analysis_silent(
                    &app_state,
                    RunCandidateScoringInput {
                        candidate_id: candidate.id,
                        job_id: candidate.job_id,
                        run_id: Some(format!("{run_id_for_task}-{pending_id}")),
                    },
                )?;

                Ok(PendingSyncItemResult {
                    pending_candidate_id: pending_id,
                    status: "SYNCED".to_string(),
                    candidate_id: Some(candidate.id),
                    error_code: None,
                    error_message: None,
                })
            })();

            match item_result {
                Ok(item) => {
                    success += 1;
                    outcomes.push(item);
                }
                Err(error) => {
                    failed += 1;
                    let conn = open_connection(&app_state.db_path)
                        .map_err(|db_error| db_error.to_string())?;
                    let now = now_iso();
                    conn.execute(
                        r#"
                        UPDATE pending_candidates
                        SET sync_status = 'FAILED',
                            sync_error_code = 'pending_ai_sync_failed',
                            sync_error_message = ?1,
                            updated_at = ?2
                        WHERE id = ?3
                        "#,
                        params![error.clone(), now, pending_id],
                    )
                    .map_err(|db_error| db_error.to_string())?;
                    outcomes.push(PendingSyncItemResult {
                        pending_candidate_id: pending_id,
                        status: "FAILED".to_string(),
                        candidate_id: current_candidate_id,
                        error_code: Some("pending_ai_sync_failed".to_string()),
                        error_message: Some(error),
                    });
                }
            }

            completed += 1;
            let current = outcomes.last().cloned();
            emit_pending_sync_progress(
                &app_handle_for_task,
                PendingSyncProgressEventPayload {
                    run_id: run_id_for_task.clone(),
                    total,
                    completed,
                    success,
                    failed,
                    current_pending_candidate_id: current
                        .as_ref()
                        .map(|item| item.pending_candidate_id),
                    current_candidate_id: current.as_ref().and_then(|item| item.candidate_id),
                    current_status: current.as_ref().map(|item| item.status.clone()),
                    message: if failed > 0 && completed == total {
                        format!("同步完成，成功 {success}，失败 {failed}")
                    } else {
                        format!("同步进度 {completed}/{total}")
                    },
                    at: now_iso(),
                },
            );
        }

        Ok(PendingSyncRunResult {
            run_id: run_id_for_task,
            total,
            completed,
            success,
            failed,
            outcomes,
        })
    })
    .await
    .map_err(|error| format!("pending_ai_sync_task_join_error:{error}"))?
}

#[tauri::command]
pub(crate) fn move_candidate_stage(
    state: State<'_, AppState>,
    input: MoveStageInput,
) -> Result<PipelineEvent, String> {
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

    if !is_valid_transition(&current_stage_text, input.to_stage.as_db()) {
        return Err(AppError::InvalidTransition {
            from: current_stage_text,
            to: input.to_stage.as_db().to_string(),
        }
        .to_string());
    }

    let now = now_iso();
    conn.execute(
        "UPDATE candidates SET stage = ?1, updated_at = ?2 WHERE id = ?3",
        params![input.to_stage.as_db(), now, input.candidate_id],
    )
    .map_err(|error| error.to_string())?;

    if let Some(job_id) = input.job_id {
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
                input.to_stage.as_db(),
                input.note,
                now,
                now,
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    conn.execute(
        r#"
        INSERT INTO pipeline_events(candidate_id, job_id, from_stage, to_stage, note, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![
            input.candidate_id,
            input.job_id,
            current_stage_text,
            input.to_stage.as_db(),
            input.note,
            now,
        ],
    )
    .map_err(|error| error.to_string())?;

    let event_id = conn.last_insert_rowid();
    let event = conn
        .query_row(
            "SELECT id, candidate_id, job_id, from_stage, to_stage, note, created_at FROM pipeline_events WHERE id = ?1",
            [event_id],
            |row| {
                let from_stage_text: String = row.get(3)?;
                let to_stage_text: String = row.get(4)?;
                Ok(PipelineEvent {
                    id: row.get(0)?,
                    candidate_id: row.get(1)?,
                    job_id: row.get(2)?,
                    from_stage: PipelineStage::from_db(&from_stage_text).map_err(|err| {
                        rusqlite::Error::FromSqlConversionFailure(
                            3,
                            rusqlite::types::Type::Text,
                            Box::new(err),
                        )
                    })?,
                    to_stage: PipelineStage::from_db(&to_stage_text).map_err(|err| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(err),
                        )
                    })?,
                    note: row.get(5)?,
                    created_at: row.get(6)?,
                })
            },
        )
        .map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "candidate.stage.move",
        "candidate",
        Some(input.candidate_id.to_string()),
        serde_json::json!({"toStage": input.to_stage, "jobId": input.job_id}),
    )
    .map_err(|error| error.to_string())?;

    Ok(event)
}

#[tauri::command]
pub(crate) fn list_pipeline_events(
    state: State<'_, AppState>,
    candidate_id: i64,
) -> Result<Vec<PipelineEvent>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, candidate_id, job_id, from_stage, to_stage, note, created_at FROM pipeline_events WHERE candidate_id = ?1 ORDER BY created_at DESC",
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([candidate_id], |row| {
            let from_stage_text: String = row.get(3)?;
            let to_stage_text: String = row.get(4)?;
            Ok(PipelineEvent {
                id: row.get(0)?,
                candidate_id: row.get(1)?,
                job_id: row.get(2)?,
                from_stage: PipelineStage::from_db(&from_stage_text).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?,
                to_stage: PipelineStage::from_db(&to_stage_text).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?,
                note: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn resume_record_from_row(row: &rusqlite::Row<'_>) -> Result<ResumeRecord, rusqlite::Error> {
    let parsed_text: String = row.get(4)?;
    let parsed = serde_json::from_str(&parsed_text).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(err))
    })?;

    Ok(ResumeRecord {
        id: row.get(0)?,
        candidate_id: row.get(1)?,
        source: row.get(2)?,
        raw_text: row.get(3)?,
        parsed,
        original_file_name: row.get(7)?,
        original_file_content_type: row.get(8)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

#[tauri::command]
pub(crate) fn preview_resume_profile(
    input: PreviewResumeProfileInput,
) -> Result<ResumeProfilePreview, String> {
    let file_name = input.file_name.trim();
    if file_name.is_empty() {
        return Err("resume_file_name_required".to_string());
    }
    let content = BASE64_STANDARD
        .decode(input.content_base64.trim())
        .map_err(|error| error.to_string())?;
    if content.is_empty() {
        return Err("resume_file_content_empty".to_string());
    }

    let extracted =
        extract_resume_content_from_bytes(file_name, &content, input.enable_ocr.unwrap_or(true))?;
    let profile = extract_resume_profile_fields(&extracted.plain_text);
    let _ = input.content_type;

    Ok(ResumeProfilePreview {
        full_text: extracted.canonical_markdown,
        extracted: profile,
        warnings: extracted.warnings,
        content_format: extracted.content_format,
        source_extension: extracted.extension,
    })
}

fn read_resume_record(
    conn: &Connection,
    candidate_id: i64,
) -> Result<Option<ResumeRecord>, String> {
    conn.query_row(
        r#"
        SELECT r.id, r.candidate_id, r.source, r.raw_text, r.parsed_json,
               r.created_at, r.updated_at, rf.file_name, rf.content_type
        FROM resumes r
        LEFT JOIN resume_files rf ON rf.candidate_id = r.candidate_id
        WHERE r.candidate_id = ?1
        "#,
        [candidate_id],
        resume_record_from_row,
    )
    .optional()
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn get_resume(
    state: State<'_, AppState>,
    candidate_id: i64,
) -> Result<Option<ResumeRecord>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    read_resume_record(&conn, candidate_id)
}

#[tauri::command]
pub(crate) fn delete_resume(state: State<'_, AppState>, candidate_id: i64) -> Result<bool, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let deleted_resume_files = conn
        .execute(
            "DELETE FROM resume_files WHERE candidate_id = ?1",
            params![candidate_id],
        )
        .map_err(|error| error.to_string())?;
    let deleted_resumes = conn
        .execute(
            "DELETE FROM resumes WHERE candidate_id = ?1",
            params![candidate_id],
        )
        .map_err(|error| error.to_string())?;

    sync_candidate_search(&conn, candidate_id).map_err(|error| error.to_string())?;

    if deleted_resume_files > 0 || deleted_resumes > 0 {
        write_audit(
            &conn,
            "resume.delete",
            "resume",
            Some(candidate_id.to_string()),
            serde_json::json!({"candidateId": candidate_id}),
        )
        .map_err(|error| error.to_string())?;
        return Ok(true);
    }

    Ok(false)
}

#[tauri::command]
pub(crate) fn upsert_resume(
    state: State<'_, AppState>,
    input: UpsertResumeInput,
) -> Result<ResumeRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let now = now_iso();
    let source = input
        .source
        .unwrap_or(SourceType::Manual)
        .as_db()
        .to_string();
    let parser_v3_enabled = resume_parser_v3_enabled();
    let enable_ocr = input.enable_ocr.unwrap_or(true);

    let decoded_original_file = if let Some(original_file) = input.original_file.as_ref() {
        let content = BASE64_STANDARD
            .decode(original_file.content_base64.trim())
            .map_err(|error| error.to_string())?;
        let file_name = original_file.file_name.trim().to_string();
        let content_type = original_file
            .content_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        Some((file_name, content_type, content))
    } else {
        None
    };

    let mut raw_text = input.raw_text.unwrap_or_default();
    let mut parsed_value = input.parsed.filter(ResumeParsedV2::is_v2_json);

    if parsed_value.is_none() && raw_text.trim().is_empty() {
        if let Some((file_name, _, content)) = decoded_original_file.as_ref() {
            let extracted = if parser_v3_enabled {
                extract_resume_content_from_bytes(file_name, content, enable_ocr)?
            } else {
                let (text, ocr_used, extension) =
                    extract_resume_text_from_bytes(file_name, content, enable_ocr)?;
                ResumeTextExtraction {
                    canonical_markdown: text.clone(),
                    plain_text: text,
                    extension,
                    ocr_used,
                    warnings: Vec::new(),
                    content_format: "plain".to_string(),
                }
            };

            raw_text = extracted.canonical_markdown.clone();
            let mut parsed =
                parse_resume_text_v2(&extracted.plain_text, &source, extracted.ocr_used, None);
            parsed.parse_meta.content_format = extracted.content_format;
            parsed.parse_meta.source_extension = Some(extracted.extension);
            parsed.parse_meta.warnings = extracted.warnings;
            parsed_value = Some(parsed.to_value());
        }
    }

    let parsed_value = parsed_value
        .unwrap_or_else(|| parse_resume_text_v2(&raw_text, &source, false, None).to_value());
    let parsed_json = parsed_value.to_string();

    conn.execute(
        r#"
        INSERT INTO resumes(candidate_id, source, raw_text, parsed_json, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(candidate_id)
        DO UPDATE SET source = excluded.source, raw_text = excluded.raw_text, parsed_json = excluded.parsed_json, updated_at = excluded.updated_at
        "#,
        params![
            input.candidate_id,
            source,
            raw_text,
            parsed_json,
            now,
            now,
        ],
    )
    .map_err(|error| error.to_string())?;

    if let Some((file_name, content_type, content)) = decoded_original_file.as_ref() {
        if !file_name.trim().is_empty() && !content.is_empty() {
            conn.execute(
                r#"
                INSERT INTO resume_files(candidate_id, file_name, content_type, content_blob, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(candidate_id)
                DO UPDATE SET
                    file_name = excluded.file_name,
                    content_type = excluded.content_type,
                    content_blob = excluded.content_blob,
                    updated_at = excluded.updated_at
                "#,
                params![
                    input.candidate_id,
                    file_name,
                    content_type.clone(),
                    content,
                    now,
                    now,
                ],
            )
            .map_err(|error| error.to_string())?;
        }
    }

    sync_candidate_search(&conn, input.candidate_id).map_err(|error| error.to_string())?;

    let record = read_resume_record(&conn, input.candidate_id)?
        .ok_or_else(|| "resume_upsert_read_back_missing".to_string())?;

    write_audit(
        &conn,
        "resume.upsert",
        "resume",
        Some(record.id.to_string()),
        serde_json::json!({"candidateId": record.candidate_id, "source": record.source}),
    )
    .map_err(|error| error.to_string())?;

    Ok(record)
}

#[cfg(test)]
fn run_candidate_analysis_blocking<F>(
    state: &AppState,
    input: RunAnalysisInput,
    on_progress: F,
) -> Result<AnalysisRecord, String>
where
    F: FnMut(AnalysisProgressUpdate),
{
    let mut on_progress = on_progress;
    on_progress(analysis_progress_update(
        "prepare",
        "running",
        "start",
        "开始读取候选人与简历上下文",
        None,
    ));

    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;

    let candidate = conn
        .query_row(
            "SELECT id, years_of_experience, stage, tags_json FROM candidates WHERE id = ?1",
            [input.candidate_id],
            |row| {
                let tags_json: String = row.get(3)?;
                let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, String>(2)?,
                    tags,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Candidate {} not found", input.candidate_id))?;

    let materialized_resume = ensure_resume_materialized(&conn, input.candidate_id)?;

    let mut required_skills: Vec<String> = Vec::new();
    let mut max_salary: Option<f64> = None;
    let mut min_years: f64 = 0.0;

    if let Some(job_id) = input.job_id {
        if let Some((description, salary_k)) = conn
            .query_row(
                "SELECT description, salary_k FROM jobs WHERE id = ?1",
                [job_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| error.to_string())?
        {
            if let Some(description_text) = description {
                required_skills = description_text
                    .split(|char: char| !char.is_alphanumeric() && char != '+')
                    .filter(|token| token.len() >= 3)
                    .take(8)
                    .map(|token| token.to_lowercase())
                    .collect();
            }

            if let Some(salary_text) = salary_k {
                let numeric = salary_text
                    .split('-')
                    .last()
                    .and_then(|item| item.parse::<f64>().ok());
                max_salary = numeric;
            }
        }
    }

    let skills = parse_skills_from_parsed_json(&materialized_resume.parsed_value);
    let normalized_skills: Vec<String> = skills.iter().map(|skill| skill.to_lowercase()).collect();
    on_progress(analysis_progress_update(
        "prepare",
        "running",
        "progress",
        "已提取候选人技能与岗位关键词",
        Some(serde_json::json!({
            "skillCount": skills.len(),
            "requiredKeywordCount": required_skills.len(),
        })),
    ));

    let matched = required_skills
        .iter()
        .filter(|required| {
            normalized_skills
                .iter()
                .any(|owned| owned.contains(*required))
        })
        .count() as i32;

    let skill_score = if required_skills.is_empty() {
        75
    } else {
        clamp_score((matched * 100) / required_skills.len() as i32)
    };

    let experience_score = clamp_score((candidate.1 * 12.0) as i32 + 20);
    min_years = min_years.max((required_skills.len() as f64 / 2.0).floor());

    let compensation_score = if let Some(max) = max_salary {
        let expected = expected_salary_k_from_parsed_json(&materialized_resume.parsed_value)
            .unwrap_or(max - 5.0);
        clamp_score((80.0 + (max - expected) * 3.0) as i32)
    } else {
        75
    };

    let stability_score = clamp_score(60 + (candidate.1 / 2.0 * 10.0) as i32);

    let dimension_scores = vec![
        DimensionScore {
            key: "skill_match".to_string(),
            score: skill_score,
            reason: format!(
                "Matched {} out of {} extracted role keywords.",
                matched,
                required_skills.len()
            ),
        },
        DimensionScore {
            key: "experience".to_string(),
            score: experience_score,
            reason: format!(
                "Candidate experience {:.1} years, role baseline {:.1} years.",
                candidate.1, min_years
            ),
        },
        DimensionScore {
            key: "compensation".to_string(),
            score: compensation_score,
            reason: "Compensation fit estimated from available profile fields.".to_string(),
        },
        DimensionScore {
            key: "stability".to_string(),
            score: stability_score,
            reason: format!(
                "Current stage {} with {} profile tags.",
                candidate.2,
                candidate.3.len()
            ),
        },
    ];

    let mut risks = Vec::<String>::new();
    if skill_score < 60 {
        risks.push("核心技能覆盖不足，建议补充技术验证。".to_string());
    }
    if compensation_score < 60 {
        risks.push("薪资期望与岗位预算可能存在偏差。".to_string());
    }

    let mut highlights = Vec::<String>::new();
    if skill_score >= 70 {
        highlights.push("技能匹配度较高，可进入技术面。".to_string());
    }
    if experience_score >= 75 {
        highlights.push("工作年限满足岗位要求。".to_string());
    }

    let suggestions = if risks.is_empty() {
        vec!["建议尽快安排首轮面试，验证业务场景适配度。".to_string()]
    } else {
        vec!["面试中重点核实风险项，并追加结构化评分。".to_string()]
    };

    let evidence = vec![
        EvidenceItem {
            dimension: "skill_match".to_string(),
            statement: format!("Skills extracted: {}", skills.join(", ")),
            source_snippet: materialized_resume.raw_text.chars().take(140).collect(),
        },
        EvidenceItem {
            dimension: "experience".to_string(),
            statement: format!("Years of experience: {:.1}", candidate.1),
            source_snippet: materialized_resume.raw_text.chars().take(140).collect(),
        },
    ];

    let local_overall_score = clamp_score(
        (dimension_scores[0].score as f64 * 0.4
            + dimension_scores[1].score as f64 * 0.25
            + dimension_scores[2].score as f64 * 0.15
            + dimension_scores[3].score as f64 * 0.2)
            .round() as i32,
    );

    let local_payload = AiAnalysisPayload {
        overall_score: local_overall_score,
        dimension_scores,
        risks,
        highlights,
        suggestions,
        evidence,
        confidence: None,
    };

    let prompt_context = AiPromptContext {
        required_skills,
        extracted_skills: skills,
        candidate_years: candidate.1,
        expected_salary_k: expected_salary_k_from_parsed_json(&materialized_resume.parsed_value),
        max_salary_k: max_salary,
        stage: candidate.2,
        tags: candidate.3,
        resume_raw_text: materialized_resume.raw_text.clone(),
        resume_parsed: materialized_resume.parsed_value.clone(),
    };

    let ai_settings =
        resolve_ai_settings(&conn, &state.cipher).map_err(|error| error.to_string())?;
    let resume_attachment = materialized_resume
        .attachment
        .clone()
        .or(read_resume_attachment(&conn, input.candidate_id)?);
    let provider_name = ai_settings.provider.as_db().to_string();
    let model_name = ai_settings.model.clone();
    let input_mode =
        planned_resume_input_mode(&ai_settings, resume_attachment.as_ref()).to_string();
    on_progress(analysis_progress_update(
        "ai",
        "running",
        "start",
        format!("开始调用模型 {} / {}", provider_name, model_name),
        Some(serde_json::json!({
            "provider": provider_name,
            "model": model_name,
        })),
    ));

    let cloud_result = {
        let mut ai_hook = |event: AiInvokeProgressEvent| match event {
            AiInvokeProgressEvent::AttemptStart { attempt, total } => {
                on_progress(analysis_progress_update(
                    "ai",
                    "running",
                    "progress",
                    format!("模型调用进行中（第 {attempt}/{total} 次）"),
                    Some(serde_json::json!({
                        "attempt": attempt,
                        "total": total,
                    })),
                ));
            }
            AiInvokeProgressEvent::AttemptFailure {
                attempt,
                total,
                error,
            } => {
                on_progress(analysis_progress_update(
                    "ai",
                    "running",
                    "retry",
                    format!("第 {attempt}/{total} 次调用失败，准备重试：{error}"),
                    Some(serde_json::json!({
                        "attempt": attempt,
                        "total": total,
                    })),
                ));
            }
            AiInvokeProgressEvent::Parsed { confidence } => {
                on_progress(analysis_progress_update(
                    "ai",
                    "running",
                    "progress",
                    "模型响应已解析，正在整理可解释摘要",
                    Some(serde_json::json!({
                        "confidence": confidence,
                    })),
                ));
            }
        };
        invoke_cloud_provider(
            &ai_settings,
            &prompt_context,
            &local_payload,
            resume_attachment.as_ref(),
            Some(&mut ai_hook),
        )
    };
    let (final_payload, model_info) = match cloud_result {
        Ok(payload) => (
            payload.clone(),
            serde_json::json!({
                "provider": provider_name,
                "model": model_name,
                "generatedAt": now_iso(),
                "mode": "cloud",
                "input_mode": input_mode,
                "confidence": payload.confidence,
            }),
        ),
        Err(reason) => (
            local_payload.clone(),
            serde_json::json!({
                "provider": provider_name,
                "model": model_name,
                "generatedAt": now_iso(),
                "mode": "fallback",
                "input_mode": input_mode,
                "fallbackReason": reason,
            }),
        ),
    };
    on_progress(analysis_progress_update(
        "ai",
        "running",
        "summary",
        format!("综合评分 {} 分", final_payload.overall_score),
        None,
    ));
    for item in final_payload.dimension_scores.iter().take(2) {
        on_progress(analysis_progress_update(
            "ai",
            "running",
            "summary",
            format!("{}：{}", item.key, item.reason),
            None,
        ));
    }
    if let Some(highlight) = final_payload.highlights.first() {
        on_progress(analysis_progress_update(
            "ai",
            "running",
            "summary",
            format!("亮点：{}", highlight),
            None,
        ));
    }
    if let Some(risk) = final_payload.risks.first() {
        on_progress(analysis_progress_update(
            "ai",
            "running",
            "summary",
            format!("风险：{}", risk),
            None,
        ));
    }
    on_progress(analysis_progress_update(
        "persist",
        "running",
        "start",
        "正在写入分析结果并刷新视图",
        None,
    ));

    let created_at = now_iso();
    conn.execute(
        r#"
        INSERT INTO analysis_results(
            candidate_id, job_id, overall_score, dimension_scores_json,
            risks_json, highlights_json, suggestions_json, evidence_json,
            model_info_json, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            input.candidate_id,
            input.job_id,
            final_payload.overall_score,
            serde_json::to_string(&final_payload.dimension_scores)
                .map_err(|error| error.to_string())?,
            serde_json::to_string(&final_payload.risks).map_err(|error| error.to_string())?,
            serde_json::to_string(&final_payload.highlights).map_err(|error| error.to_string())?,
            serde_json::to_string(&final_payload.suggestions).map_err(|error| error.to_string())?,
            serde_json::to_string(&final_payload.evidence).map_err(|error| error.to_string())?,
            model_info.to_string(),
            created_at,
        ],
    )
    .map_err(|error| error.to_string())?;
    on_progress(analysis_progress_update(
        "persist",
        "running",
        "progress",
        "分析结果已写入，正在记录审计日志",
        None,
    ));

    let id = conn.last_insert_rowid();

    let result = AnalysisRecord {
        id,
        candidate_id: input.candidate_id,
        job_id: input.job_id,
        overall_score: final_payload.overall_score,
        dimension_scores: final_payload.dimension_scores,
        risks: final_payload.risks,
        highlights: final_payload.highlights,
        suggestions: final_payload.suggestions,
        evidence: final_payload.evidence,
        model_info,
        created_at,
    };

    write_audit(
        &conn,
        "analysis.run",
        "analysis_result",
        Some(result.id.to_string()),
        serde_json::json!({"candidateId": input.candidate_id, "jobId": input.job_id}),
    )
    .map_err(|error| error.to_string())?;

    Ok(result)
}

#[cfg(test)]
#[tauri::command]
pub(crate) async fn run_candidate_analysis(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    input: RunAnalysisInput,
) -> Result<AnalysisRecord, String> {
    let run_id = input
        .run_id
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("analysis-{}-{}", input.candidate_id, now_iso()));
    let candidate_id = input.candidate_id;
    let app_state = state.inner().clone();
    let app_handle_for_task = app_handle.clone();
    let run_id_for_task = run_id.clone();
    let input_for_task = input.clone();

    let task_result = tauri::async_runtime::spawn_blocking(move || {
        let mut last_phase = "prepare".to_string();
        let result = run_candidate_analysis_blocking(&app_state, input_for_task, |update| {
            last_phase = update.phase.to_string();
            emit_analysis_progress(&app_handle_for_task, &run_id_for_task, candidate_id, update);
        });
        (result, last_phase)
    })
    .await
    .map_err(|error| {
        let message = format!("analysis_task_join_error: {error}");
        emit_analysis_progress(
            &app_handle,
            &run_id,
            candidate_id,
            analysis_progress_update("persist", "failed", "end", message.clone(), None),
        );
        message
    })?;

    let (result, last_phase) = task_result;
    match result {
        Ok(record) => {
            emit_analysis_progress(
                &app_handle,
                &run_id,
                candidate_id,
                analysis_progress_update(
                    "persist",
                    "completed",
                    "end",
                    "分析完成并已刷新结果",
                    None,
                ),
            );
            Ok(record)
        }
        Err(error) => {
            let phase = if last_phase == "ai" {
                "ai"
            } else if last_phase == "persist" {
                "persist"
            } else {
                "prepare"
            };
            emit_analysis_progress(
                &app_handle,
                &run_id,
                candidate_id,
                analysis_progress_update(phase, "failed", "end", error.clone(), None),
            );
            Err(error)
        }
    }
}

#[tauri::command]
pub(crate) fn list_analysis(
    state: State<'_, AppState>,
    candidate_id: i64,
) -> Result<Vec<AnalysisRecord>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, candidate_id, job_id, overall_score, dimension_scores_json,
                   risks_json, highlights_json, suggestions_json, evidence_json,
                   model_info_json, created_at
            FROM analysis_results
            WHERE candidate_id = ?1
            ORDER BY created_at DESC
            "#,
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([candidate_id], |row| {
            let parse_vec = |index: usize| -> Result<Vec<String>, rusqlite::Error> {
                let text: String = row.get(index)?;
                serde_json::from_str(&text).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        index,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })
            };

            let dimension_text: String = row.get(4)?;
            let evidence_text: String = row.get(8)?;
            let model_info_text: String = row.get(9)?;

            let dimension_scores: Vec<DimensionScore> = serde_json::from_str(&dimension_text)
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
            let evidence: Vec<EvidenceItem> =
                serde_json::from_str(&evidence_text).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        8,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
            let model_info = serde_json::from_str(&model_info_text).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    9,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;

            Ok(AnalysisRecord {
                id: row.get(0)?,
                candidate_id: row.get(1)?,
                job_id: row.get(2)?,
                overall_score: row.get(3)?,
                dimension_scores,
                risks: parse_vec(5)?,
                highlights: parse_vec(6)?,
                suggestions: parse_vec(7)?,
                evidence,
                model_info,
                created_at: row.get(10)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}
