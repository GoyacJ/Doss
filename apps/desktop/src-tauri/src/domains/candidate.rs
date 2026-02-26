use super::super::*;

fn read_candidate_by_id(
    conn: &Connection,
    candidate_id: i64,
    cipher: &FieldCipher,
) -> Result<Candidate, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, external_id, source, name, current_company, score, age, gender, years_of_experience, stage, tags_json, phone_enc, email_enc, created_at, updated_at FROM candidates WHERE id = ?1",
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

    conn.execute(
        r#"
        INSERT INTO candidates(
            external_id, source, name, current_company, score, age, gender, years_of_experience, stage,
            phone_enc, phone_hash, email_enc, email_hash, tags_json, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'NEW', ?9, ?10, ?11, ?12, ?13, ?14, ?15)
        "#,
        params![
            input.external_id,
            input.source.unwrap_or(SourceType::Manual).as_db(),
            input.name,
            input.current_company,
            score,
            age,
            gender,
            input.years_of_experience,
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
                tags_json = ?7,
                phone_enc = CASE WHEN ?8 IS NOT NULL THEN ?8 ELSE phone_enc END,
                phone_hash = CASE WHEN ?9 IS NOT NULL THEN ?9 ELSE phone_hash END,
                email_enc = CASE WHEN ?10 IS NOT NULL THEN ?10 ELSE email_enc END,
                email_hash = CASE WHEN ?11 IS NOT NULL THEN ?11 ELSE email_hash END,
                updated_at = ?12
            WHERE id = ?13
            "#,
            params![
                name,
                current_company,
                input.years_of_experience.max(0.0),
                score,
                age,
                gender,
                tags_json,
                phone_enc,
                phone_hash,
                email_enc,
                email_hash,
                now,
                input.candidate_id,
            ],
        )
        .map_err(|error| error.to_string())?;

    if affected == 0 {
        return Err(format!("Candidate {} not found", input.candidate_id));
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

    let existing_tags_json = existing
        .ok_or_else(|| format!("Candidate {} not found", input.candidate_id))?;
    let existing_tags: Vec<String> = serde_json::from_str(&existing_tags_json).unwrap_or_default();
    let incoming_tags = input.tags.unwrap_or_default();
    let merged_tags = merge_candidate_tags(&existing_tags, &incoming_tags);
    let merged_tags_json = serde_json::to_string(&merged_tags).map_err(|error| error.to_string())?;

    let incoming_company = input
        .current_company
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
                tags_json = ?3,
                phone_enc = COALESCE(phone_enc, ?4),
                phone_hash = COALESCE(phone_hash, ?5),
                email_enc = COALESCE(email_enc, ?6),
                email_hash = COALESCE(email_hash, ?7),
                updated_at = ?8
            WHERE id = ?9
            "#,
            params![
                incoming_company,
                incoming_years,
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
                "SELECT id, external_id, source, name, current_company, score, age, gender, years_of_experience, stage, tags_json, phone_enc, email_enc, created_at, updated_at FROM candidates WHERE stage = ?1 ORDER BY updated_at DESC",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map([filter_stage.as_db()], |row| candidate_from_row(row, &state.cipher))
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    } else {
        let mut stmt = conn
            .prepare(
                "SELECT id, external_id, source, name, current_company, score, age, gender, years_of_experience, stage, tags_json, phone_enc, email_enc, created_at, updated_at FROM candidates ORDER BY updated_at DESC",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map([], |row| candidate_from_row(row, &state.cipher))
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }
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
        return Err(
            AppError::InvalidTransition {
                from: current_stage_text,
                to: input.to_stage.as_db().to_string(),
            }
            .to_string(),
        );
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

#[tauri::command]
pub(crate) fn upsert_resume(
    state: State<'_, AppState>,
    input: UpsertResumeInput,
) -> Result<ResumeRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let now = now_iso();
    let source = input.source.unwrap_or(SourceType::Manual).as_db().to_string();
    let parsed_json = input.parsed.to_string();

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
            input.raw_text,
            parsed_json,
            now,
            now,
        ],
    )
    .map_err(|error| error.to_string())?;

    sync_candidate_search(&conn, input.candidate_id).map_err(|error| error.to_string())?;

    let record = conn
        .query_row(
            "SELECT id, candidate_id, source, raw_text, parsed_json, created_at, updated_at FROM resumes WHERE candidate_id = ?1",
            [input.candidate_id],
            |row| {
                let parsed_text: String = row.get(4)?;
                let parsed = serde_json::from_str(&parsed_text).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?;
                Ok(ResumeRecord {
                    id: row.get(0)?,
                    candidate_id: row.get(1)?,
                    source: row.get(2)?,
                    raw_text: row.get(3)?,
                    parsed,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .map_err(|error| error.to_string())?;

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

#[tauri::command]
pub(crate) fn parse_resume_file(
    input: ParseResumeFileInput,
) -> Result<ParseResumeFileOutput, String> {
    let bytes = BASE64_STANDARD
        .decode(input.content_base64.trim())
        .map_err(|error| error.to_string())?;

    let extension = extract_file_extension(&input.file_name);
    let enable_ocr = input.enable_ocr.unwrap_or(false);

    let mut raw_text = match extension.as_str() {
        "pdf" => extract_text_from_pdf_bytes(&bytes),
        "docx" => extract_text_from_docx_bytes(&bytes),
        "txt" | "md" => extract_text_from_plain_bytes(&bytes),
        "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff" => Ok(String::new()),
        _ => Err(format!("unsupported_resume_file_type: {}", extension)),
    }?;

    let mut ocr_used = false;
    if enable_ocr
        && raw_text.trim().is_empty()
        && matches!(
            extension.as_str(),
            "pdf" | "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff"
        )
    {
        if let Ok(ocr_text) = try_tesseract_ocr(&bytes, &extension) {
            if !ocr_text.trim().is_empty() {
                raw_text = ocr_text;
                ocr_used = true;
            }
        }
    }

    if raw_text.trim().is_empty() {
        return Err("resume_text_empty_after_parse".to_string());
    }

    let normalized = normalize_resume_text(&raw_text);
    let parsed = build_structured_resume_fields(&normalized);

    Ok(ParseResumeFileOutput {
        raw_text: normalized,
        parsed,
        metadata: serde_json::json!({
            "fileName": input.file_name,
            "extension": extension,
            "size": bytes.len(),
            "ocrUsed": ocr_used,
        }),
    })
}

#[tauri::command]
pub(crate) fn run_candidate_analysis(
    state: State<'_, AppState>,
    input: RunAnalysisInput,
) -> Result<AnalysisRecord, String> {
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

    let resume_row: (String, Value) = conn
        .query_row(
            "SELECT raw_text, parsed_json FROM resumes WHERE candidate_id = ?1",
            [input.candidate_id],
            |row| {
                let parsed_json_text: String = row.get(1)?;
                let parsed_json = serde_json::from_str(&parsed_json_text).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?;
                Ok((row.get(0)?, parsed_json))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Resume required before analysis".to_string())?;

    let mut required_skills: Vec<String> = Vec::new();
    let mut max_salary: Option<f64> = None;
    let mut min_years: f64 = 0.0;

    if let Some(job_id) = input.job_id {
        if let Some((description, salary_k)) = conn
            .query_row(
                "SELECT description, salary_k FROM jobs WHERE id = ?1",
                [job_id],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?)),
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

    let skills = parse_skills(&resume_row.1);
    let normalized_skills: Vec<String> = skills.iter().map(|skill| skill.to_lowercase()).collect();

    let matched = required_skills
        .iter()
        .filter(|required| normalized_skills.iter().any(|owned| owned.contains(*required)))
        .count() as i32;

    let skill_score = if required_skills.is_empty() {
        75
    } else {
        clamp_score((matched * 100) / required_skills.len() as i32)
    };

    let experience_score = clamp_score((candidate.1 * 12.0) as i32 + 20);
    min_years = min_years.max((required_skills.len() as f64 / 2.0).floor());

    let compensation_score = if let Some(max) = max_salary {
        let expected = resume_row
            .1
            .get("expectedSalaryK")
            .and_then(|value| value.as_f64())
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
            source_snippet: resume_row.0.chars().take(140).collect(),
        },
        EvidenceItem {
            dimension: "experience".to_string(),
            statement: format!("Years of experience: {:.1}", candidate.1),
            source_snippet: resume_row.0.chars().take(140).collect(),
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
        expected_salary_k: resume_row
            .1
            .get("expectedSalaryK")
            .and_then(|value| value.as_f64()),
        max_salary_k: max_salary,
        stage: candidate.2,
        tags: candidate.3,
        resume_raw_text: resume_row.0,
        resume_parsed: resume_row.1,
    };

    let ai_settings = resolve_ai_settings(&conn, &state.cipher).map_err(|error| error.to_string())?;
    let provider_name = ai_settings.provider.as_db().to_string();
    let model_name = ai_settings.model.clone();

    let cloud_result = invoke_cloud_provider(&ai_settings, &prompt_context, &local_payload);
    let (final_payload, model_info) = match cloud_result {
        Ok(payload) => (
            payload.clone(),
            serde_json::json!({
                "provider": provider_name,
                "model": model_name,
                "generatedAt": now_iso(),
                "mode": "cloud",
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
                "fallbackReason": reason,
            }),
        ),
    };

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
            serde_json::to_string(&final_payload.dimension_scores).map_err(|error| error.to_string())?,
            serde_json::to_string(&final_payload.risks).map_err(|error| error.to_string())?,
            serde_json::to_string(&final_payload.highlights).map_err(|error| error.to_string())?,
            serde_json::to_string(&final_payload.suggestions).map_err(|error| error.to_string())?,
            serde_json::to_string(&final_payload.evidence).map_err(|error| error.to_string())?,
            model_info.to_string(),
            created_at,
        ],
    )
    .map_err(|error| error.to_string())?;

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

            let dimension_scores = serde_json::from_str(&dimension_text).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    4,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            let evidence = serde_json::from_str(&evidence_text).map_err(|error| {
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
