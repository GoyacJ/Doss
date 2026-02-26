use super::super::*;

#[tauri::command]
pub(crate) fn create_crawl_task(
    state: State<'_, AppState>,
    input: NewCrawlTaskInput,
) -> Result<CrawlTask, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let now = now_iso();
    conn.execute(
        r#"
        INSERT INTO crawl_tasks(source, mode, task_type, status, retry_count, error_code, payload_json, snapshot_json, started_at, finished_at, created_at, updated_at)
        VALUES (?1, ?2, ?3, 'PENDING', 0, NULL, ?4, NULL, NULL, NULL, ?5, ?6)
        "#,
        params![
            input.source.as_db(),
            input.mode.as_db(),
            input.task_type,
            input.payload.to_string(),
            now,
            now,
        ],
    )
    .map_err(|error| error.to_string())?;

    let id = conn.last_insert_rowid();

    let task = conn
        .query_row(
            "SELECT id, source, mode, task_type, status, retry_count, error_code, payload_json, snapshot_json, started_at, finished_at, created_at, updated_at FROM crawl_tasks WHERE id = ?1",
            [id],
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
                    started_at: row.get(9)?,
                    finished_at: row.get(10)?,
                    created_at: row.get(11)?,
                    updated_at: row.get(12)?,
                })
            },
        )
        .map_err(|error| error.to_string())?;

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

    conn.execute(
        r#"
        UPDATE crawl_tasks
        SET status = ?1,
            retry_count = COALESCE(?2, retry_count),
            error_code = ?3,
            snapshot_json = ?4,
            started_at = COALESCE(?5, started_at),
            finished_at = COALESCE(?6, finished_at),
            updated_at = ?7
        WHERE id = ?8
        "#,
        params![
            input.status.as_db(),
            input.retry_count,
            input.error_code,
            input.snapshot.map(|value| value.to_string()),
            started_at,
            finished_at,
            now,
            input.task_id,
        ],
    )
    .map_err(|error| error.to_string())?;

    let task = conn
        .query_row(
            "SELECT id, source, mode, task_type, status, retry_count, error_code, payload_json, snapshot_json, started_at, finished_at, created_at, updated_at FROM crawl_tasks WHERE id = ?1",
            [input.task_id],
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
                    started_at: row.get(9)?,
                    finished_at: row.get(10)?,
                    created_at: row.get(11)?,
                    updated_at: row.get(12)?,
                })
            },
        )
        .map_err(|error| error.to_string())?;

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
            "SELECT id, source, mode, task_type, status, retry_count, error_code, payload_json, snapshot_json, started_at, finished_at, created_at, updated_at FROM crawl_tasks ORDER BY updated_at DESC",
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
                started_at: row.get(9)?,
                finished_at: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}
