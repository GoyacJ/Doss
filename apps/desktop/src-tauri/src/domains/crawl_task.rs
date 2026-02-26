use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use tauri::State;

use crate::core::pii::hash_value;
use crate::core::state::AppState;
use crate::core::time::now_iso;
use crate::infra::audit::write_audit;
use crate::infra::db::open_connection;
use crate::models::common::CrawlTaskStatus;
use crate::models::crawl::{
    CrawlTask, CrawlTaskPerson, NewCrawlTaskInput, UpdateCrawlTaskInput,
    UpdateCrawlTaskPeopleSyncInput, UpsertCrawlTaskPeopleInput,
};

fn read_task_by_id(conn: &Connection, task_id: i64) -> Result<CrawlTask, String> {
    conn.query_row(
        "SELECT id, source, mode, task_type, status, retry_count, error_code, payload_json, snapshot_json, schedule_type, schedule_time, schedule_day, next_run_at, started_at, finished_at, created_at, updated_at FROM crawl_tasks WHERE id = ?1",
        [task_id],
        |row| {
            let payload_text: String = row.get(7)?;
            let snapshot_text: Option<String> = row.get(8)?;
            Ok(CrawlTask {
                id: row.get(0)?,
                source: row.get(1)?,
                mode: row.get(2)?,
                task_type: row.get(3)?,
                status: row.get(4)?,
                retry_count: row.get(5)?,
                error_code: row.get(6)?,
                payload: serde_json::from_str(&payload_text).unwrap_or(Value::Null),
                snapshot: snapshot_text.and_then(|value| serde_json::from_str(&value).ok()),
                schedule_type: row.get(9)?,
                schedule_time: row.get(10)?,
                schedule_day: row.get(11)?,
                next_run_at: row.get(12)?,
                started_at: row.get(13)?,
                finished_at: row.get(14)?,
                created_at: row.get(15)?,
                updated_at: row.get(16)?,
            })
        },
    )
    .map_err(|error| error.to_string())
}

fn normalize_sync_status(input: Option<&str>) -> String {
    let normalized = input.unwrap_or("UNSYNCED").trim().to_uppercase();
    if matches!(normalized.as_str(), "UNSYNCED" | "SYNCED" | "FAILED") {
        normalized
    } else {
        "UNSYNCED".to_string()
    }
}

fn normalize_schedule_type(value: Option<&str>) -> &'static str {
    match value
        .map(|raw| raw.trim().to_uppercase())
        .unwrap_or_else(|| "ONCE".to_string())
        .as_str()
    {
        "DAILY" => "DAILY",
        "MONTHLY" => "MONTHLY",
        _ => "ONCE",
    }
}

fn build_person_dedupe_key(
    source: &str,
    external_id: Option<&str>,
    name: &str,
    company: Option<&str>,
    years: f64,
) -> String {
    if let Some(external_id_value) = external_id {
        let trimmed = external_id_value.trim();
        if !trimmed.is_empty() {
            return format!("{source}:{trimmed}");
        }
    }

    let seed = format!(
        "{}|{}|{}|{}",
        source.trim().to_lowercase(),
        name.trim().to_lowercase(),
        company.unwrap_or("").trim().to_lowercase(),
        years.max(0.0)
    );
    format!("{source}:sha256:{}", hash_value(&seed))
}

fn read_task_people_by_task(
    conn: &Connection,
    task_id: i64,
) -> Result<Vec<CrawlTaskPerson>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, task_id, source, external_id, name, current_company, years_of_experience, sync_status, sync_error_code, sync_error_message, candidate_id, created_at, updated_at FROM crawl_task_people WHERE task_id = ?1 ORDER BY updated_at DESC, id DESC",
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([task_id], |row| {
            Ok(CrawlTaskPerson {
                id: row.get(0)?,
                task_id: row.get(1)?,
                source: row.get(2)?,
                external_id: row.get(3)?,
                name: row.get(4)?,
                current_company: row.get(5)?,
                years_of_experience: row.get(6)?,
                sync_status: row.get(7)?,
                sync_error_code: row.get(8)?,
                sync_error_message: row.get(9)?,
                candidate_id: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn create_crawl_task(
    state: State<'_, AppState>,
    input: NewCrawlTaskInput,
) -> Result<CrawlTask, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let now = now_iso();
    let schedule_type = normalize_schedule_type(input.schedule_type.as_deref());
    conn.execute(
        r#"
        INSERT INTO crawl_tasks(
            source, mode, task_type, status, retry_count, error_code, payload_json, snapshot_json,
            schedule_type, schedule_time, schedule_day, next_run_at,
            started_at, finished_at, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, 'PENDING', 0, NULL, ?4, NULL, ?5, ?6, ?7, ?8, NULL, NULL, ?9, ?10)
        "#,
        params![
            input.source.as_db(),
            input.mode.as_db(),
            input.task_type,
            input.payload.to_string(),
            schedule_type,
            input.schedule_time,
            input.schedule_day,
            input.next_run_at,
            now,
            now
        ],
    )
    .map_err(|error| error.to_string())?;

    let id = conn.last_insert_rowid();
    let task = read_task_by_id(&conn, id)?;

    write_audit(
        &conn,
        "crawl_task.create",
        "crawl_task",
        Some(task.id.to_string()),
        serde_json::json!({"source": task.source, "mode": task.mode, "taskType": task.task_type}),
    )
    .map_err(|error| error.to_string())?;

    Ok(task)
}

#[tauri::command]
pub(crate) fn update_crawl_task(
    state: State<'_, AppState>,
    input: UpdateCrawlTaskInput,
) -> Result<CrawlTask, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let now = now_iso();

    let started_at: Option<String> = if matches!(input.status, CrawlTaskStatus::Running) {
        Some(now.clone())
    } else {
        conn.query_row(
            "SELECT started_at FROM crawl_tasks WHERE id = ?1",
            [input.task_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .flatten()
    };

    let finished_at = if matches!(
        input.status,
        CrawlTaskStatus::Failed | CrawlTaskStatus::Succeeded
    ) {
        Some(now.clone())
    } else {
        None
    };
    let schedule_type = input
        .schedule_type
        .as_deref()
        .map(|value| normalize_schedule_type(Some(value)).to_string());

    conn.execute(
        r#"
        UPDATE crawl_tasks
        SET status = ?1,
            retry_count = COALESCE(?2, retry_count),
            error_code = ?3,
            snapshot_json = ?4,
            schedule_type = COALESCE(?5, schedule_type),
            schedule_time = COALESCE(?6, schedule_time),
            schedule_day = COALESCE(?7, schedule_day),
            next_run_at = COALESCE(?8, next_run_at),
            started_at = COALESCE(?9, started_at),
            finished_at = COALESCE(?10, finished_at),
            updated_at = ?11
        WHERE id = ?12
        "#,
        params![
            input.status.as_db(),
            input.retry_count,
            input.error_code,
            input.snapshot.map(|value| value.to_string()),
            schedule_type,
            input.schedule_time,
            input.schedule_day,
            input.next_run_at,
            started_at,
            finished_at,
            now,
            input.task_id,
        ],
    )
    .map_err(|error| error.to_string())?;

    let task = read_task_by_id(&conn, input.task_id)?;

    write_audit(
        &conn,
        "crawl_task.update",
        "crawl_task",
        Some(task.id.to_string()),
        serde_json::json!({"status": task.status, "retryCount": task.retry_count, "errorCode": task.error_code}),
    )
    .map_err(|error| error.to_string())?;

    Ok(task)
}

#[tauri::command]
pub(crate) fn list_crawl_tasks(state: State<'_, AppState>) -> Result<Vec<CrawlTask>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, source, mode, task_type, status, retry_count, error_code, payload_json, snapshot_json, schedule_type, schedule_time, schedule_day, next_run_at, started_at, finished_at, created_at, updated_at FROM crawl_tasks ORDER BY updated_at DESC",
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            let payload_text: String = row.get(7)?;
            let snapshot_text: Option<String> = row.get(8)?;
            Ok(CrawlTask {
                id: row.get(0)?,
                source: row.get(1)?,
                mode: row.get(2)?,
                task_type: row.get(3)?,
                status: row.get(4)?,
                retry_count: row.get(5)?,
                error_code: row.get(6)?,
                payload: serde_json::from_str(&payload_text).unwrap_or(Value::Null),
                snapshot: snapshot_text.and_then(|value| serde_json::from_str(&value).ok()),
                schedule_type: row.get(9)?,
                schedule_time: row.get(10)?,
                schedule_day: row.get(11)?,
                next_run_at: row.get(12)?,
                started_at: row.get(13)?,
                finished_at: row.get(14)?,
                created_at: row.get(15)?,
                updated_at: row.get(16)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn delete_crawl_task(state: State<'_, AppState>, task_id: i64) -> Result<bool, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;

    let affected = conn
        .execute("DELETE FROM crawl_tasks WHERE id = ?1", [task_id])
        .map_err(|error| error.to_string())?;

    if affected == 0 {
        return Err(format!("Crawl task {} not found", task_id));
    }

    write_audit(
        &conn,
        "crawl_task.delete",
        "crawl_task",
        Some(task_id.to_string()),
        serde_json::json!({"deleted": true}),
    )
    .map_err(|error| error.to_string())?;

    Ok(true)
}

#[tauri::command]
pub(crate) fn upsert_crawl_task_people(
    state: State<'_, AppState>,
    input: UpsertCrawlTaskPeopleInput,
) -> Result<Vec<CrawlTaskPerson>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;

    let exists = conn
        .query_row(
            "SELECT id FROM crawl_tasks WHERE id = ?1",
            [input.task_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if exists.is_none() {
        return Err(format!("Crawl task {} not found", input.task_id));
    }

    let now = now_iso();
    for person in input.people {
        let source = person.source.as_db().to_string();
        if source == "all" || source == "manual" {
            return Err("crawl_task_person_source_invalid".to_string());
        }

        let name = person.name.trim();
        if name.is_empty() {
            continue;
        }

        let sync_status = normalize_sync_status(person.sync_status.as_deref());
        let dedupe_key = build_person_dedupe_key(
            &source,
            person.external_id.as_deref(),
            name,
            person.current_company.as_deref(),
            person.years_of_experience,
        );

        conn.execute(
            r#"
            INSERT INTO crawl_task_people(
                task_id, source, dedupe_key, external_id, name, current_company, years_of_experience,
                sync_status, sync_error_code, sync_error_message, candidate_id, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(task_id, dedupe_key)
            DO UPDATE SET
                external_id = excluded.external_id,
                name = excluded.name,
                current_company = excluded.current_company,
                years_of_experience = excluded.years_of_experience,
                sync_status = CASE
                    WHEN crawl_task_people.sync_status = 'SYNCED' AND excluded.sync_status = 'UNSYNCED'
                    THEN crawl_task_people.sync_status
                    ELSE excluded.sync_status
                END,
                sync_error_code = CASE
                    WHEN crawl_task_people.sync_status = 'SYNCED' AND excluded.sync_status = 'UNSYNCED'
                    THEN crawl_task_people.sync_error_code
                    ELSE excluded.sync_error_code
                END,
                sync_error_message = CASE
                    WHEN crawl_task_people.sync_status = 'SYNCED' AND excluded.sync_status = 'UNSYNCED'
                    THEN crawl_task_people.sync_error_message
                    ELSE excluded.sync_error_message
                END,
                candidate_id = COALESCE(crawl_task_people.candidate_id, excluded.candidate_id),
                updated_at = excluded.updated_at
            "#,
            params![
                input.task_id,
                source,
                dedupe_key,
                person.external_id,
                name,
                person.current_company,
                person.years_of_experience.max(0.0),
                sync_status,
                person.sync_error_code,
                person.sync_error_message,
                person.candidate_id,
                now,
                now,
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    let people = read_task_people_by_task(&conn, input.task_id)?;
    write_audit(
        &conn,
        "crawl_task_people.upsert",
        "crawl_task",
        Some(input.task_id.to_string()),
        serde_json::json!({"count": people.len()}),
    )
    .map_err(|error| error.to_string())?;

    Ok(people)
}

#[tauri::command]
pub(crate) fn list_crawl_task_people(
    state: State<'_, AppState>,
    task_id: i64,
) -> Result<Vec<CrawlTaskPerson>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    read_task_people_by_task(&conn, task_id)
}

#[tauri::command]
pub(crate) fn update_crawl_task_people_sync(
    state: State<'_, AppState>,
    input: UpdateCrawlTaskPeopleSyncInput,
) -> Result<Vec<CrawlTaskPerson>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let now = now_iso();

    for update in input.updates {
        let sync_status = normalize_sync_status(Some(update.sync_status.as_str()));
        conn.execute(
            r#"
            UPDATE crawl_task_people
            SET sync_status = ?1,
                sync_error_code = ?2,
                sync_error_message = ?3,
                candidate_id = ?4,
                updated_at = ?5
            WHERE id = ?6 AND task_id = ?7
            "#,
            params![
                sync_status,
                update.sync_error_code,
                update.sync_error_message,
                update.candidate_id,
                now,
                update.person_id,
                input.task_id,
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    let people = read_task_people_by_task(&conn, input.task_id)?;
    write_audit(
        &conn,
        "crawl_task_people.sync.update",
        "crawl_task",
        Some(input.task_id.to_string()),
        serde_json::json!({"count": people.len()}),
    )
    .map_err(|error| error.to_string())?;

    Ok(people)
}
