use rusqlite::{params, Connection};

use crate::core::error::AppResult;

pub(crate) fn sync_candidate_search(conn: &Connection, candidate_id: i64) -> AppResult<()> {
    let mut stmt = conn.prepare(
        r#"
        SELECT c.name, c.tags_json, COALESCE(r.raw_text, '')
        FROM candidates c
        LEFT JOIN resumes r ON r.candidate_id = c.id
        WHERE c.id = ?1
        "#,
    )?;

    let (name, tags_json, raw_text): (String, String, String) = stmt
        .query_row([candidate_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;

    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
    let tags_text = tags.join(" ");

    conn.execute(
        "DELETE FROM candidate_search WHERE candidate_id = ?1",
        [candidate_id],
    )?;
    conn.execute(
        "INSERT INTO candidate_search(candidate_id, name, tags, raw_text) VALUES (?1, ?2, ?3, ?4)",
        params![candidate_id, name, tags_text, raw_text],
    )?;

    Ok(())
}
