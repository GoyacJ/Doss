use rusqlite::{params, Connection};
use tauri::State;

use crate::core::state::AppState;
use crate::core::time::now_iso;
use crate::infra::audit::write_audit;
use crate::infra::db::open_connection;
use crate::models::common::SourceType;
use crate::models::job::{Job, NewJobInput, UpdateJobInput};

#[tauri::command]
pub(crate) fn create_job(state: State<'_, AppState>, input: NewJobInput) -> Result<Job, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let now = now_iso();
    let source = input
        .source
        .unwrap_or(SourceType::Manual)
        .as_db()
        .to_string();
    let title = input.title.trim().to_string();
    let company = input.company.trim().to_string();
    if title.is_empty() || company.is_empty() {
        return Err("job_title_or_company_required".to_string());
    }

    let city = input
        .city
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let salary_k = input
        .salary_k
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let description = input
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    conn.execute(
        r#"
        INSERT INTO jobs(external_id, source, title, company, city, salary_k, description, status, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'ACTIVE', ?8, ?9)
        "#,
        params![
            input.external_id,
            source,
            title,
            company,
            city,
            salary_k,
            description,
            now,
            now,
        ],
    )
    .map_err(|error| error.to_string())?;

    let id = conn.last_insert_rowid();
    let job = read_job_by_id(&conn, id)?;

    write_audit(
        &conn,
        "job.create",
        "job",
        Some(job.id.to_string()),
        serde_json::json!({"source": job.source, "title": job.title}),
    )
    .map_err(|error| error.to_string())?;

    Ok(job)
}

pub(crate) fn read_job_by_id(conn: &Connection, job_id: i64) -> Result<Job, String> {
    conn.query_row(
        r#"
        SELECT
            j.id,
            j.external_id,
            j.source,
            j.title,
            j.company,
            j.city,
            j.salary_k,
            j.description,
            COALESCE(j.status, 'ACTIVE'),
            jo.template_id,
            st.name,
            j.created_at,
            j.updated_at
        FROM jobs j
        LEFT JOIN job_scoring_overrides jo ON jo.job_id = j.id
        LEFT JOIN scoring_templates st ON st.id = jo.template_id
        WHERE j.id = ?1
        "#,
        [job_id],
        |row| {
            Ok(Job {
                id: row.get(0)?,
                external_id: row.get(1)?,
                source: row.get(2)?,
                title: row.get(3)?,
                company: row.get(4)?,
                city: row.get(5)?,
                salary_k: row.get(6)?,
                description: row.get(7)?,
                status: row.get(8)?,
                scoring_template_id: row.get(9)?,
                scoring_template_name: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn update_job(state: State<'_, AppState>, input: UpdateJobInput) -> Result<Job, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let title = input.title.trim().to_string();
    let company = input.company.trim().to_string();
    if title.is_empty() || company.is_empty() {
        return Err("job_title_or_company_required".to_string());
    }

    let city = input
        .city
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let salary_k = input
        .salary_k
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let description = input
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let now = now_iso();

    let affected = conn
        .execute(
            r#"
            UPDATE jobs
            SET title = ?1,
                company = ?2,
                city = ?3,
                salary_k = ?4,
                description = ?5,
                updated_at = ?6
            WHERE id = ?7
            "#,
            params![
                title,
                company,
                city,
                salary_k,
                description,
                now,
                input.job_id
            ],
        )
        .map_err(|error| error.to_string())?;

    if affected == 0 {
        return Err(format!("Job {} not found", input.job_id));
    }

    let job = read_job_by_id(&conn, input.job_id)?;
    write_audit(
        &conn,
        "job.update",
        "job",
        Some(job.id.to_string()),
        serde_json::json!({
            "title": job.title,
            "company": job.company,
            "status": job.status,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(job)
}

#[tauri::command]
pub(crate) fn stop_job(state: State<'_, AppState>, job_id: i64) -> Result<Job, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let now = now_iso();
    let affected = conn
        .execute(
            "UPDATE jobs SET status = 'STOPPED', updated_at = ?1 WHERE id = ?2",
            params![now, job_id],
        )
        .map_err(|error| error.to_string())?;

    if affected == 0 {
        return Err(format!("Job {} not found", job_id));
    }

    let job = read_job_by_id(&conn, job_id)?;
    write_audit(
        &conn,
        "job.stop",
        "job",
        Some(job.id.to_string()),
        serde_json::json!({
            "status": job.status,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(job)
}

#[tauri::command]
pub(crate) fn delete_job(state: State<'_, AppState>, job_id: i64) -> Result<bool, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let active_task_count = count_active_crawl_tasks_for_job(&conn, job_id)?;
    if active_task_count > 0 {
        return Err(format!(
            "该职位存在 {active_task_count} 个执行中的任务，请先停止任务后再删除"
        ));
    }

    let affected = conn
        .execute("DELETE FROM jobs WHERE id = ?1", [job_id])
        .map_err(|error| error.to_string())?;

    if affected > 0 {
        write_audit(
            &conn,
            "job.delete",
            "job",
            Some(job_id.to_string()),
            serde_json::json!({}),
        )
        .map_err(|error| error.to_string())?;
    }

    Ok(affected > 0)
}

pub(crate) fn count_active_crawl_tasks_for_job(
    conn: &Connection,
    job_id: i64,
) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM crawl_tasks WHERE status IN ('PENDING', 'RUNNING', 'PAUSED') AND CAST(json_extract(payload_json, '$.localJobId') AS INTEGER) = ?1",
        [job_id],
        |row| row.get::<_, i64>(0),
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn list_jobs(state: State<'_, AppState>) -> Result<Vec<Job>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT
                j.id,
                j.external_id,
                j.source,
                j.title,
                j.company,
                j.city,
                j.salary_k,
                j.description,
                COALESCE(j.status, 'ACTIVE'),
                jo.template_id,
                st.name,
                j.created_at,
                j.updated_at
            FROM jobs j
            LEFT JOIN job_scoring_overrides jo ON jo.job_id = j.id
            LEFT JOIN scoring_templates st ON st.id = jo.template_id
            ORDER BY j.updated_at DESC
            "#,
        )
        .map_err(|error| error.to_string())?;

    let jobs = stmt
        .query_map([], |row| {
            Ok(Job {
                id: row.get(0)?,
                external_id: row.get(1)?,
                source: row.get(2)?,
                title: row.get(3)?,
                company: row.get(4)?,
                city: row.get(5)?,
                salary_k: row.get(6)?,
                description: row.get(7)?,
                status: row.get(8)?,
                scoring_template_id: row.get(9)?,
                scoring_template_name: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(jobs)
}
