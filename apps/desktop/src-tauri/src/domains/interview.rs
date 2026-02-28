use base64::prelude::*;
use rusqlite::{params, OptionalExtension};
use serde_json::Value;
use std::fs;
use std::path::Path;
use tauri::State;

use crate::core::state::AppState;
use crate::core::time::now_iso;
use crate::domains::screening::{
    build_generated_interview_questions, build_interview_slot_key,
    evaluate_interview_feedback_payload, normalize_interview_questions, parse_job_required_skills,
    parse_skills,
};
use crate::infra::audit::write_audit;
use crate::infra::db::open_connection;
use crate::models::interview::{
    GenerateInterviewKitInput, InterviewEvaluationRecord, InterviewFeedbackRecord,
    InterviewKitRecord, InterviewQuestion, RunInterviewEvaluationInput, SaveInterviewKitInput,
    SaveInterviewRecordingInput, SaveInterviewRecordingOutput, SubmitInterviewFeedbackInput,
};

fn safe_recording_file_stem(file_name: &str) -> String {
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("interview")
        .trim();
    let normalized = stem
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if normalized.is_empty() {
        "interview".to_string()
    } else {
        normalized
    }
}

fn safe_recording_extension(file_name: &str) -> &'static str {
    match Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.trim().to_lowercase())
        .as_deref()
    {
        Some("wav") => "wav",
        Some("mp3") => "mp3",
        Some("m4a") => "m4a",
        Some("aac") => "aac",
        Some("ogg") => "ogg",
        _ => "webm",
    }
}

#[tauri::command]
pub(crate) fn generate_interview_kit(
    state: State<'_, AppState>,
    input: GenerateInterviewKitInput,
) -> Result<InterviewKitRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;

    let (candidate_name, years_of_experience) = conn
        .query_row(
            "SELECT name, years_of_experience FROM candidates WHERE id = ?1",
            [input.candidate_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Candidate {} not found", input.candidate_id))?;

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

    let mut role_title: Option<String> = None;
    let mut required_skills = Vec::<String>::new();
    if let Some(job_id) = effective_job_id {
        if let Some((title, description)) = conn
            .query_row(
                "SELECT title, description FROM jobs WHERE id = ?1",
                [job_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?
        {
            role_title = Some(title);
            if let Some(description_text) = description {
                required_skills = parse_job_required_skills(&description_text);
            }
        }
    }

    let resume_parsed = conn
        .query_row(
            "SELECT parsed_json FROM resumes WHERE candidate_id = ?1",
            [input.candidate_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or(Value::Null);
    let extracted_skills = parse_skills(&resume_parsed);

    let screening_hint = conn
        .query_row(
            "SELECT recommendation, risk_level FROM scoring_results WHERE candidate_id = ?1 ORDER BY created_at DESC LIMIT 1",
            [input.candidate_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let latest_analysis_risks = conn
        .query_row(
            "SELECT structured_result_json FROM scoring_results WHERE candidate_id = ?1 ORDER BY created_at DESC LIMIT 1",
            [input.candidate_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| {
            value
                .get("risks")
                .and_then(|item| item.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .map(str::trim)
                        .filter(|item| !item.is_empty())
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
        })
        .unwrap_or_default();

    let questions = build_generated_interview_questions(
        role_title.as_deref(),
        &candidate_name,
        years_of_experience,
        &required_skills,
        &extracted_skills,
        screening_hint.as_ref().map(|value| value.0.as_str()),
        screening_hint.as_ref().map(|value| value.1.as_str()),
        &latest_analysis_risks,
    );
    let now = now_iso();
    write_audit(
        &conn,
        "interview.kit.generate",
        "interview_kit",
        Some(input.candidate_id.to_string()),
        serde_json::json!({
            "candidateId": input.candidate_id,
            "jobId": effective_job_id,
            "questionCount": questions.len(),
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(InterviewKitRecord {
        id: None,
        candidate_id: input.candidate_id,
        job_id: effective_job_id,
        questions,
        generated_by: "rule-engine-v1".to_string(),
        created_at: now.clone(),
        updated_at: now,
    })
}

#[tauri::command]
pub(crate) fn save_interview_kit(
    state: State<'_, AppState>,
    input: SaveInterviewKitInput,
) -> Result<InterviewKitRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    conn.query_row(
        "SELECT id FROM candidates WHERE id = ?1",
        [input.candidate_id],
        |row| row.get::<_, i64>(0),
    )
    .optional()
    .map_err(|error| error.to_string())?
    .ok_or_else(|| format!("Candidate {} not found", input.candidate_id))?;

    let normalized_questions = normalize_interview_questions(input.questions)?;
    let slot_key = build_interview_slot_key(input.candidate_id, input.job_id);
    let generated_by = "rule-engine-v1".to_string();
    let now = now_iso();
    let content_json = serde_json::json!({
        "questions": normalized_questions
    })
    .to_string();

    conn.execute(
        r#"
        INSERT INTO interview_kits(slot_key, candidate_id, job_id, content_json, generated_by, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(slot_key)
        DO UPDATE SET
            candidate_id = excluded.candidate_id,
            job_id = excluded.job_id,
            content_json = excluded.content_json,
            generated_by = excluded.generated_by,
            updated_at = excluded.updated_at
        "#,
        params![
            slot_key,
            input.candidate_id,
            input.job_id,
            content_json,
            generated_by,
            now,
            now,
        ],
    )
    .map_err(|error| error.to_string())?;

    let (id, saved_generated_by, created_at, updated_at) = conn
        .query_row(
            "SELECT id, generated_by, created_at, updated_at FROM interview_kits WHERE slot_key = ?1",
            [build_interview_slot_key(input.candidate_id, input.job_id)],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .map_err(|error| error.to_string())?;

    let saved_questions = conn
        .query_row(
            "SELECT content_json FROM interview_kits WHERE id = ?1",
            [id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| error.to_string())
        .and_then(|content_text| {
            let content: Value =
                serde_json::from_str(&content_text).map_err(|error| error.to_string())?;
            serde_json::from_value::<Vec<InterviewQuestion>>(
                content.get("questions").cloned().unwrap_or(Value::Null),
            )
            .map_err(|error| error.to_string())
        })?;

    let record = InterviewKitRecord {
        id: Some(id),
        candidate_id: input.candidate_id,
        job_id: input.job_id,
        questions: saved_questions,
        generated_by: saved_generated_by,
        created_at,
        updated_at,
    };

    write_audit(
        &conn,
        "interview.kit.save",
        "interview_kit",
        Some(id.to_string()),
        serde_json::json!({
            "candidateId": record.candidate_id,
            "jobId": record.job_id,
            "questionCount": record.questions.len(),
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(record)
}

#[tauri::command]
pub(crate) fn save_interview_recording(
    state: State<'_, AppState>,
    input: SaveInterviewRecordingInput,
) -> Result<SaveInterviewRecordingOutput, String> {
    let bytes = BASE64_STANDARD
        .decode(input.content_base64.trim())
        .map_err(|error| error.to_string())?;
    if bytes.is_empty() {
        return Err("interview_recording_empty".to_string());
    }

    let created_at = now_iso();
    let file_stem = safe_recording_file_stem(&input.file_name);
    let extension = safe_recording_extension(&input.file_name);
    let recordings_dir = state
        .db_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("interview-recordings");
    fs::create_dir_all(&recordings_dir).map_err(|error| error.to_string())?;

    let file_name = format!(
        "{}-{}.{extension}",
        file_stem,
        created_at.replace([':', '.'], "-")
    );
    let file_path = recordings_dir.join(file_name);
    fs::write(&file_path, &bytes).map_err(|error| error.to_string())?;

    Ok(SaveInterviewRecordingOutput {
        recording_path: file_path.to_string_lossy().to_string(),
        size: bytes.len(),
        created_at,
    })
}

#[tauri::command]
pub(crate) fn submit_interview_feedback(
    state: State<'_, AppState>,
    input: SubmitInterviewFeedbackInput,
) -> Result<InterviewFeedbackRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    conn.query_row(
        "SELECT id FROM candidates WHERE id = ?1",
        [input.candidate_id],
        |row| row.get::<_, i64>(0),
    )
    .optional()
    .map_err(|error| error.to_string())?
    .ok_or_else(|| format!("Candidate {} not found", input.candidate_id))?;

    let transcript_text = input.transcript_text.trim().to_string();
    if transcript_text.is_empty() {
        return Err("interview_transcript_required".to_string());
    }
    if !input.structured_feedback.is_object() {
        return Err("interview_structured_feedback_required".to_string());
    }

    let recording_path = input
        .recording_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let now = now_iso();
    conn.execute(
        r#"
        INSERT INTO interview_feedback(
            candidate_id, job_id, transcript_text, structured_feedback_json,
            recording_path, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            input.candidate_id,
            input.job_id,
            transcript_text,
            input.structured_feedback.to_string(),
            recording_path,
            now,
            now,
        ],
    )
    .map_err(|error| error.to_string())?;

    let id = conn.last_insert_rowid();
    let record = conn
        .query_row(
            r#"
            SELECT id, candidate_id, job_id, transcript_text, structured_feedback_json,
                   recording_path, created_at, updated_at
            FROM interview_feedback
            WHERE id = ?1
            "#,
            [id],
            |row| {
                let structured_text: String = row.get(4)?;
                let structured_feedback = serde_json::from_str(&structured_text)
                    .unwrap_or(Value::Object(Default::default()));
                Ok(InterviewFeedbackRecord {
                    id: row.get(0)?,
                    candidate_id: row.get(1)?,
                    job_id: row.get(2)?,
                    transcript_text: row.get(3)?,
                    structured_feedback,
                    recording_path: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        )
        .map_err(|error| error.to_string())?;

    write_audit(
        &conn,
        "interview.feedback.submit",
        "interview_feedback",
        Some(record.id.to_string()),
        serde_json::json!({
            "candidateId": record.candidate_id,
            "jobId": record.job_id,
            "transcriptLength": record.transcript_text.chars().count(),
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(record)
}

#[tauri::command]
pub(crate) fn run_interview_evaluation(
    state: State<'_, AppState>,
    input: RunInterviewEvaluationInput,
) -> Result<InterviewEvaluationRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;

    let parse_feedback_row = |row: &rusqlite::Row<'_>| -> Result<
        (i64, i64, Option<i64>, String, Value),
        rusqlite::Error,
    > {
        let structured_text: String = row.get(4)?;
        let structured_feedback = serde_json::from_str(&structured_text).map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(err))
        })?;
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Option<i64>>(2)?,
            row.get::<_, String>(3)?,
            structured_feedback,
        ))
    };

    let feedback = if let Some(feedback_id) = input.feedback_id {
        conn.query_row(
            r#"
            SELECT id, candidate_id, job_id, transcript_text, structured_feedback_json
            FROM interview_feedback
            WHERE id = ?1
            "#,
            [feedback_id],
            parse_feedback_row,
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("interview_feedback {} not found", feedback_id))?
    } else if let Some(job_id) = input.job_id {
        conn.query_row(
            r#"
            SELECT id, candidate_id, job_id, transcript_text, structured_feedback_json
            FROM interview_feedback
            WHERE candidate_id = ?1 AND job_id = ?2
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            params![input.candidate_id, job_id],
            parse_feedback_row,
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "interview_feedback_not_found".to_string())?
    } else {
        conn.query_row(
            r#"
            SELECT id, candidate_id, job_id, transcript_text, structured_feedback_json
            FROM interview_feedback
            WHERE candidate_id = ?1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            [input.candidate_id],
            parse_feedback_row,
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "interview_feedback_not_found".to_string())?
    };

    if feedback.1 != input.candidate_id {
        return Err("feedback_candidate_mismatch".to_string());
    }

    let payload = evaluate_interview_feedback_payload(&feedback.3, &feedback.4);
    let effective_job_id = input.job_id.or(feedback.2);
    let created_at = now_iso();
    conn.execute(
        r#"
        INSERT INTO interview_evaluations(
            candidate_id, job_id, feedback_id, recommendation, overall_score, confidence,
            evidence_json, verification_points_json, uncertainty, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            input.candidate_id,
            effective_job_id,
            feedback.0,
            payload.recommendation,
            payload.overall_score,
            payload.confidence,
            serde_json::to_string(&payload.evidence).map_err(|error| error.to_string())?,
            serde_json::to_string(&payload.verification_points)
                .map_err(|error| error.to_string())?,
            payload.uncertainty,
            created_at,
        ],
    )
    .map_err(|error| error.to_string())?;

    let id = conn.last_insert_rowid();
    let record = InterviewEvaluationRecord {
        id,
        candidate_id: input.candidate_id,
        job_id: effective_job_id,
        feedback_id: feedback.0,
        recommendation: payload.recommendation.clone(),
        overall_score: payload.overall_score,
        confidence: payload.confidence,
        evidence: payload.evidence.clone(),
        verification_points: payload.verification_points.clone(),
        uncertainty: payload.uncertainty.clone(),
        created_at,
    };

    write_audit(
        &conn,
        "interview.evaluation.run",
        "interview_evaluation",
        Some(record.id.to_string()),
        serde_json::json!({
            "candidateId": record.candidate_id,
            "jobId": record.job_id,
            "feedbackId": record.feedback_id,
            "recommendation": record.recommendation,
            "overallScore": record.overall_score,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(record)
}

#[tauri::command]
pub(crate) fn list_interview_evaluations(
    state: State<'_, AppState>,
    candidate_id: i64,
) -> Result<Vec<InterviewEvaluationRecord>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, candidate_id, job_id, feedback_id, recommendation, overall_score, confidence,
                   evidence_json, verification_points_json, uncertainty, created_at
            FROM interview_evaluations
            WHERE candidate_id = ?1
            ORDER BY created_at DESC
            "#,
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([candidate_id], |row| {
            let evidence_text: String = row.get(7)?;
            let verification_text: String = row.get(8)?;
            Ok(InterviewEvaluationRecord {
                id: row.get(0)?,
                candidate_id: row.get(1)?,
                job_id: row.get(2)?,
                feedback_id: row.get(3)?,
                recommendation: row.get(4)?,
                overall_score: row.get(5)?,
                confidence: row.get(6)?,
                evidence: serde_json::from_str(&evidence_text).unwrap_or_default(),
                verification_points: serde_json::from_str(&verification_text).unwrap_or_default(),
                uncertainty: row.get(9)?,
                created_at: row.get(10)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}
