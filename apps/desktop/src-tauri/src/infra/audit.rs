use rusqlite::{params, Connection};
use serde_json::Value;

use crate::core::error::AppResult;
use crate::core::time::now_iso;

pub(crate) fn write_audit(
    conn: &Connection,
    action: &str,
    entity_type: &str,
    entity_id: Option<String>,
    payload: Value,
) -> AppResult<()> {
    let created_at = now_iso();
    conn.execute(
        "INSERT INTO audit_logs(action, entity_type, entity_id, payload_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![action, entity_type, entity_id, payload.to_string(), created_at],
    )?;
    Ok(())
}
