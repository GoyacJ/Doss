use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::core::time::now_iso;
use crate::domains::ai_runtime::{read_resume_attachment, TextGenerationAttachment};
use crate::domains::resume_parser::{
    extract_file_extension, extract_resume_content_from_bytes, extract_resume_text_from_bytes,
    parse_resume_text_v2, resume_parser_v3_enabled, ResumeTextExtraction,
};
use crate::models::resume::ResumeParsedV2;

#[derive(Debug, Clone)]
pub(crate) struct MaterializedResume {
    pub(crate) raw_text: String,
    pub(crate) parsed_value: Value,
    pub(crate) attachment: Option<TextGenerationAttachment>,
}

fn read_resume_row(
    conn: &Connection,
    candidate_id: i64,
) -> Result<Option<(String, String, String)>, String> {
    conn.query_row(
        "SELECT source, raw_text, parsed_json FROM resumes WHERE candidate_id = ?1",
        [candidate_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

fn parse_json_or_null(text: &str) -> Value {
    serde_json::from_str::<Value>(text).unwrap_or(Value::Null)
}

fn persist_materialized_resume(
    conn: &Connection,
    candidate_id: i64,
    source: &str,
    raw_text: &str,
    parsed_json: &Value,
) -> Result<(), String> {
    let now = now_iso();
    conn.execute(
        r#"
        UPDATE resumes
        SET source = ?1, raw_text = ?2, parsed_json = ?3, updated_at = ?4
        WHERE candidate_id = ?5
        "#,
        params![source, raw_text, parsed_json.to_string(), now, candidate_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn extract_from_attachment(
    file_name: &str,
    bytes: &[u8],
) -> Result<ResumeTextExtraction, String> {
    if resume_parser_v3_enabled() {
        return extract_resume_content_from_bytes(file_name, bytes, true);
    }
    let (text, ocr_used, extension) = extract_resume_text_from_bytes(file_name, bytes, true)?;
    Ok(ResumeTextExtraction {
        canonical_markdown: text.clone(),
        plain_text: text,
        extension,
        ocr_used,
        warnings: Vec::new(),
        content_format: "plain".to_string(),
    })
}

pub(crate) fn ensure_resume_materialized(
    conn: &Connection,
    candidate_id: i64,
) -> Result<MaterializedResume, String> {
    let Some((source, raw_text, parsed_text)) = read_resume_row(conn, candidate_id)? else {
        return Err("Resume required before scoring".to_string());
    };

    let attachment = read_resume_attachment(conn, candidate_id)?;
    let parsed_json = parse_json_or_null(&parsed_text);

    let mut migrated = false;
    let mut final_raw_text = raw_text.clone();
    let mut final_parsed: ResumeParsedV2;

    if ResumeParsedV2::is_v2_json(&parsed_json) {
        final_parsed = ResumeParsedV2::from_value(parsed_json.clone())?;
        if final_raw_text.trim().is_empty() {
            final_raw_text = final_parsed
                .sections
                .iter()
                .map(|item| item.content.trim())
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            if !final_raw_text.trim().is_empty() {
                migrated = true;
            }
        }
        if final_raw_text.trim().is_empty() {
            if let Some(file) = attachment.as_ref() {
                let extracted = extract_from_attachment(&file.file_name, &file.bytes)?;
                if !extracted.canonical_markdown.trim().is_empty() {
                    final_raw_text = extracted.canonical_markdown.clone();
                    final_parsed = parse_resume_text_v2(
                        &extracted.plain_text,
                        &source,
                        extracted.ocr_used,
                        None,
                    );
                    final_parsed.parse_meta.content_format = extracted.content_format;
                    final_parsed.parse_meta.source_extension = Some(extracted.extension);
                    final_parsed.parse_meta.warnings = extracted.warnings;
                    migrated = true;
                }
            }
        }
    } else {
        if final_raw_text.trim().is_empty() {
            if let Some(file) = attachment.as_ref() {
                let extracted = extract_from_attachment(&file.file_name, &file.bytes)?;
                final_raw_text = extracted.canonical_markdown.clone();
                final_parsed =
                    parse_resume_text_v2(&extracted.plain_text, &source, extracted.ocr_used, None);
                final_parsed.parse_meta.content_format = extracted.content_format;
                final_parsed.parse_meta.source_extension = Some(extracted.extension);
                final_parsed.parse_meta.warnings = extracted.warnings;
                migrated = true;
            } else {
                final_parsed = parse_resume_text_v2(&final_raw_text, &source, false, None);
                migrated = true;
            }
        } else {
            final_parsed = parse_resume_text_v2(&final_raw_text, &source, false, None);
            migrated = true;
        }
    }

    if final_parsed.parse_meta.source.trim().is_empty() {
        final_parsed.parse_meta.source = source.clone();
    }
    if final_parsed
        .parse_meta
        .source_extension
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        if let Some(file) = attachment.as_ref() {
            let extension = extract_file_extension(&file.file_name);
            if !extension.is_empty() {
                final_parsed.parse_meta.source_extension = Some(extension);
            }
        }
    }

    if migrated {
        persist_materialized_resume(
            conn,
            candidate_id,
            &source,
            &final_raw_text,
            &final_parsed.to_value(),
        )?;
    }

    Ok(MaterializedResume {
        raw_text: final_raw_text,
        parsed_value: final_parsed.to_value(),
        attachment,
    })
}

pub(crate) fn materialize_resume_from_file_full_text(
    conn: &Connection,
    candidate_id: i64,
) -> Result<MaterializedResume, String> {
    let Some((source, _raw_text, _parsed_text)) = read_resume_row(conn, candidate_id)? else {
        return Err("Resume required before scoring".to_string());
    };

    let attachment = read_resume_attachment(conn, candidate_id)?
        .ok_or_else(|| "resume_file_required_for_ai_analysis".to_string())?;

    let extracted = extract_from_attachment(&attachment.file_name, &attachment.bytes)?;
    let final_raw_text = extracted.canonical_markdown.trim().to_string();
    if final_raw_text.is_empty() {
        return Err("resume_file_text_empty_after_parse".to_string());
    }

    let mut parsed = parse_resume_text_v2(&final_raw_text, &source, extracted.ocr_used, None);
    parsed.parse_meta.content_format = extracted.content_format;
    if !extracted.extension.trim().is_empty() {
        parsed.parse_meta.source_extension = Some(extracted.extension);
    }
    parsed.parse_meta.warnings = extracted.warnings;
    let parsed_value = parsed.to_value();

    persist_materialized_resume(conn, candidate_id, &source, &final_raw_text, &parsed_value)?;

    Ok(MaterializedResume {
        raw_text: final_raw_text,
        parsed_value,
        attachment: Some(attachment),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open sqlite memory");
        conn.execute_batch(
            r#"
            CREATE TABLE resumes (
                candidate_id INTEGER PRIMARY KEY,
                source TEXT NOT NULL,
                raw_text TEXT NOT NULL,
                parsed_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE resume_files (
                candidate_id INTEGER PRIMARY KEY,
                file_name TEXT NOT NULL,
                content_type TEXT,
                content_blob BLOB NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )
        .expect("create tables");
        conn
    }

    fn seed_resume_row(conn: &Connection, candidate_id: i64) {
        conn.execute(
            r#"
            INSERT INTO resumes(candidate_id, source, raw_text, parsed_json, updated_at)
            VALUES (?1, 'manual', '', '{}', '2026-03-02T00:00:00Z')
            "#,
            [candidate_id],
        )
        .expect("seed resume");
    }

    #[test]
    fn materialize_resume_from_file_full_text_requires_resume_file() {
        let conn = setup_conn();
        seed_resume_row(&conn, 1);

        let error = materialize_resume_from_file_full_text(&conn, 1).expect_err("should fail");
        assert_eq!(error, "resume_file_required_for_ai_analysis");
    }

    #[test]
    fn materialize_resume_from_file_full_text_rejects_empty_parsed_text() {
        let conn = setup_conn();
        seed_resume_row(&conn, 2);
        conn.execute(
            r#"
            INSERT INTO resume_files(candidate_id, file_name, content_type, content_blob, created_at, updated_at)
            VALUES (?1, 'resume.txt', 'text/plain', ?2, '2026-03-02T00:00:00Z', '2026-03-02T00:00:00Z')
            "#,
            params![2_i64, b" \n \n".to_vec()],
        )
        .expect("seed resume file");

        let error = materialize_resume_from_file_full_text(&conn, 2).expect_err("should fail");
        assert_eq!(error, "resume_file_text_empty_after_parse");
    }

    #[test]
    fn materialize_resume_from_file_full_text_updates_resume_with_full_text() {
        let conn = setup_conn();
        seed_resume_row(&conn, 3);
        conn.execute(
            r#"
            INSERT INTO resume_files(candidate_id, file_name, content_type, content_blob, created_at, updated_at)
            VALUES (?1, 'resume.txt', 'text/plain', ?2, '2026-03-02T00:00:00Z', '2026-03-02T00:00:00Z')
            "#,
            params![3_i64, b"Alice\nVue\nTypeScript".to_vec()],
        )
        .expect("seed resume file");

        let materialized =
            materialize_resume_from_file_full_text(&conn, 3).expect("materialize resume");
        assert!(materialized.raw_text.contains("Alice"));

        let persisted: String = conn
            .query_row(
                "SELECT raw_text FROM resumes WHERE candidate_id = ?1",
                [3_i64],
                |row| row.get(0),
            )
            .expect("read persisted raw text");
        assert!(persisted.contains("Alice"));
    }
}
