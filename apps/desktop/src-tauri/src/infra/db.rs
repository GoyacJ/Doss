use chrono::Utc;
use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

use crate::core::error::AppResult;

pub(crate) fn open_connection(db_path: &Path) -> AppResult<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch(
        "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;",
    )?;
    Ok(conn)
}

pub(crate) fn migrate_db(db_path: &Path) -> AppResult<()> {
    let conn = open_connection(db_path)?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS jobs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            external_id TEXT,
            source TEXT NOT NULL,
            title TEXT NOT NULL,
            company TEXT NOT NULL,
            city TEXT,
            salary_k TEXT,
            description TEXT,
            status TEXT NOT NULL DEFAULT 'ACTIVE',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS candidates (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            external_id TEXT,
            source TEXT NOT NULL,
            name TEXT NOT NULL,
            current_company TEXT,
            linked_job_id INTEGER,
            linked_job_title TEXT,
            score REAL,
            age INTEGER,
            gender TEXT,
            years_of_experience REAL NOT NULL DEFAULT 0,
            address TEXT,
            stage TEXT NOT NULL DEFAULT 'NEW',
            phone_enc TEXT,
            phone_hash TEXT,
            email_enc TEXT,
            email_hash TEXT,
            tags_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS applications (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            job_id INTEGER NOT NULL,
            candidate_id INTEGER NOT NULL,
            stage TEXT NOT NULL,
            notes TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(job_id, candidate_id),
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE CASCADE,
            FOREIGN KEY(candidate_id) REFERENCES candidates(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS resumes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            candidate_id INTEGER NOT NULL UNIQUE,
            source TEXT NOT NULL,
            raw_text TEXT NOT NULL,
            parsed_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(candidate_id) REFERENCES candidates(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS analysis_results (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            candidate_id INTEGER NOT NULL,
            job_id INTEGER,
            overall_score INTEGER NOT NULL,
            dimension_scores_json TEXT NOT NULL,
            risks_json TEXT NOT NULL,
            highlights_json TEXT NOT NULL,
            suggestions_json TEXT NOT NULL,
            evidence_json TEXT NOT NULL,
            model_info_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(candidate_id) REFERENCES candidates(id) ON DELETE CASCADE,
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS scoring_templates (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            scope TEXT NOT NULL,
            job_id INTEGER,
            name TEXT NOT NULL,
            config_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(scope, job_id),
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS job_scoring_overrides (
            job_id INTEGER PRIMARY KEY,
            template_id INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE CASCADE,
            FOREIGN KEY(template_id) REFERENCES scoring_templates(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS scoring_results (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            candidate_id INTEGER NOT NULL,
            job_id INTEGER,
            template_id INTEGER,
            overall_score INTEGER NOT NULL,
            overall_score_5 REAL NOT NULL,
            t0_score_5 REAL NOT NULL,
            t1_score_5 REAL NOT NULL,
            t2_score_5 REAL NOT NULL,
            t3_score_5 REAL NOT NULL,
            recommendation TEXT NOT NULL,
            risk_level TEXT NOT NULL,
            structured_result_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL,
            FOREIGN KEY(candidate_id) REFERENCES candidates(id) ON DELETE CASCADE,
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE SET NULL,
            FOREIGN KEY(template_id) REFERENCES scoring_templates(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS screening_templates (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            scope TEXT NOT NULL,
            job_id INTEGER,
            name TEXT NOT NULL,
            config_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(scope, job_id),
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS screening_dimensions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            template_id INTEGER NOT NULL,
            dimension_key TEXT NOT NULL,
            dimension_label TEXT NOT NULL,
            weight INTEGER NOT NULL,
            sort_order INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(template_id, dimension_key),
            FOREIGN KEY(template_id) REFERENCES screening_templates(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS job_screening_overrides (
            job_id INTEGER PRIMARY KEY,
            template_id INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE CASCADE,
            FOREIGN KEY(template_id) REFERENCES screening_templates(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS screening_results (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            candidate_id INTEGER NOT NULL,
            job_id INTEGER,
            template_id INTEGER,
            t0_score REAL NOT NULL,
            t1_score INTEGER NOT NULL,
            fine_score INTEGER NOT NULL,
            bonus_score INTEGER NOT NULL,
            risk_penalty INTEGER NOT NULL,
            overall_score INTEGER NOT NULL,
            recommendation TEXT NOT NULL,
            risk_level TEXT NOT NULL,
            evidence_json TEXT NOT NULL,
            verification_points_json TEXT NOT NULL,
            structured_result_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL,
            FOREIGN KEY(candidate_id) REFERENCES candidates(id) ON DELETE CASCADE,
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE SET NULL,
            FOREIGN KEY(template_id) REFERENCES screening_templates(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS interview_kits (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            slot_key TEXT NOT NULL UNIQUE,
            candidate_id INTEGER NOT NULL,
            job_id INTEGER,
            content_json TEXT NOT NULL,
            generated_by TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(candidate_id) REFERENCES candidates(id) ON DELETE CASCADE,
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS interview_feedback (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            candidate_id INTEGER NOT NULL,
            job_id INTEGER,
            transcript_text TEXT NOT NULL,
            structured_feedback_json TEXT NOT NULL,
            recording_path TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(candidate_id) REFERENCES candidates(id) ON DELETE CASCADE,
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS interview_evaluations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            candidate_id INTEGER NOT NULL,
            job_id INTEGER,
            feedback_id INTEGER NOT NULL,
            recommendation TEXT NOT NULL,
            overall_score INTEGER NOT NULL,
            confidence REAL NOT NULL,
            evidence_json TEXT NOT NULL,
            verification_points_json TEXT NOT NULL,
            uncertainty TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(candidate_id) REFERENCES candidates(id) ON DELETE CASCADE,
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE SET NULL,
            FOREIGN KEY(feedback_id) REFERENCES interview_feedback(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS hiring_decisions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            candidate_id INTEGER NOT NULL,
            job_id INTEGER,
            interview_evaluation_id INTEGER,
            ai_recommendation TEXT,
            final_decision TEXT NOT NULL,
            reason_code TEXT NOT NULL,
            note TEXT,
            ai_deviation INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(candidate_id) REFERENCES candidates(id) ON DELETE CASCADE,
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE SET NULL,
            FOREIGN KEY(interview_evaluation_id) REFERENCES interview_evaluations(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS pipeline_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            candidate_id INTEGER NOT NULL,
            job_id INTEGER,
            from_stage TEXT NOT NULL,
            to_stage TEXT NOT NULL,
            note TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY(candidate_id) REFERENCES candidates(id) ON DELETE CASCADE,
            FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS crawl_tasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source TEXT NOT NULL,
            mode TEXT NOT NULL,
            task_type TEXT NOT NULL,
            status TEXT NOT NULL,
            retry_count INTEGER NOT NULL DEFAULT 0,
            error_code TEXT,
            payload_json TEXT NOT NULL,
            snapshot_json TEXT,
            schedule_type TEXT NOT NULL DEFAULT 'ONCE',
            schedule_time TEXT,
            schedule_day INTEGER,
            next_run_at TEXT,
            started_at TEXT,
            finished_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS crawl_task_people (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id INTEGER NOT NULL,
            source TEXT NOT NULL,
            dedupe_key TEXT NOT NULL,
            external_id TEXT,
            name TEXT NOT NULL,
            current_company TEXT,
            years_of_experience REAL NOT NULL DEFAULT 0,
            sync_status TEXT NOT NULL DEFAULT 'UNSYNCED',
            sync_error_code TEXT,
            sync_error_message TEXT,
            candidate_id INTEGER,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(task_id) REFERENCES crawl_tasks(id) ON DELETE CASCADE,
            FOREIGN KEY(candidate_id) REFERENCES candidates(id) ON DELETE SET NULL,
            UNIQUE(task_id, dedupe_key)
        );

        CREATE TABLE IF NOT EXISTS pending_candidates (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source TEXT NOT NULL DEFAULT 'manual',
            external_id TEXT,
            name TEXT NOT NULL,
            current_company TEXT,
            linked_job_id INTEGER,
            linked_job_title TEXT,
            age INTEGER,
            gender TEXT,
            years_of_experience REAL NOT NULL DEFAULT 0,
            tags_json TEXT NOT NULL DEFAULT '[]',
            phone_enc TEXT,
            phone_hash TEXT,
            email_enc TEXT,
            email_hash TEXT,
            address TEXT,
            extra_notes TEXT,
            resume_raw_text TEXT,
            resume_parsed_json TEXT NOT NULL DEFAULT '{}',
            dedupe_key TEXT NOT NULL,
            sync_status TEXT NOT NULL DEFAULT 'UNSYNCED',
            sync_error_code TEXT,
            sync_error_message TEXT,
            candidate_id INTEGER,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(candidate_id) REFERENCES candidates(id) ON DELETE SET NULL,
            UNIQUE(dedupe_key)
        );

        CREATE TABLE IF NOT EXISTS audit_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            action TEXT NOT NULL,
            entity_type TEXT NOT NULL,
            entity_id TEXT,
            payload_json TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_jobs_updated_at ON jobs(updated_at);
        CREATE INDEX IF NOT EXISTS idx_candidates_updated_at ON candidates(updated_at);
        CREATE INDEX IF NOT EXISTS idx_candidates_stage ON candidates(stage);
        CREATE INDEX IF NOT EXISTS idx_applications_job_stage ON applications(job_id, stage);
        CREATE INDEX IF NOT EXISTS idx_crawl_tasks_status ON crawl_tasks(status);
        CREATE INDEX IF NOT EXISTS idx_crawl_task_people_task ON crawl_task_people(task_id, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_crawl_task_people_sync ON crawl_task_people(task_id, sync_status);
        CREATE INDEX IF NOT EXISTS idx_pending_candidates_dedupe ON pending_candidates(dedupe_key);
        CREATE INDEX IF NOT EXISTS idx_pending_candidates_sync_status ON pending_candidates(sync_status, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_scoring_results_candidate ON scoring_results(candidate_id, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_screening_results_candidate ON screening_results(candidate_id, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_interview_kits_candidate ON interview_kits(candidate_id, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_interview_feedback_candidate ON interview_feedback(candidate_id, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_interview_evaluations_candidate ON interview_evaluations(candidate_id, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_hiring_decisions_candidate ON hiring_decisions(candidate_id, created_at DESC);

        CREATE VIRTUAL TABLE IF NOT EXISTS candidate_search USING fts5(
            candidate_id UNINDEXED,
            name,
            tags,
            raw_text
        );
        "#,
    )?;

    let _ = conn.execute(
        "ALTER TABLE jobs ADD COLUMN status TEXT NOT NULL DEFAULT 'ACTIVE'",
        [],
    );
    let _ = conn.execute("ALTER TABLE candidates ADD COLUMN age INTEGER", []);
    let _ = conn.execute("ALTER TABLE candidates ADD COLUMN gender TEXT", []);
    let _ = conn.execute("ALTER TABLE candidates ADD COLUMN score REAL", []);
    let _ = conn.execute(
        "ALTER TABLE candidates ADD COLUMN linked_job_id INTEGER",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE candidates ADD COLUMN linked_job_title TEXT",
        [],
    );
    let _ = conn.execute("ALTER TABLE candidates ADD COLUMN address TEXT", []);
    let _ = conn.execute(
        "ALTER TABLE screening_results ADD COLUMN structured_result_json TEXT NOT NULL DEFAULT '{}'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE crawl_tasks ADD COLUMN schedule_type TEXT NOT NULL DEFAULT 'ONCE'",
        [],
    );
    let _ = conn.execute("ALTER TABLE crawl_tasks ADD COLUMN schedule_time TEXT", []);
    let _ = conn.execute(
        "ALTER TABLE crawl_tasks ADD COLUMN schedule_day INTEGER",
        [],
    );
    let _ = conn.execute("ALTER TABLE crawl_tasks ADD COLUMN next_run_at TEXT", []);
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_crawl_tasks_next_run_at ON crawl_tasks(next_run_at)",
        [],
    )?;

    let _ = conn.execute(
        r#"
        UPDATE candidates
        SET linked_job_id = (
            SELECT a.job_id
            FROM applications a
            WHERE a.candidate_id = candidates.id
            ORDER BY a.updated_at DESC, a.id DESC
            LIMIT 1
        )
        WHERE linked_job_id IS NULL
        "#,
        [],
    );
    let _ = conn.execute(
        r#"
        UPDATE candidates
        SET linked_job_title = (
            SELECT j.title
            FROM applications a
            JOIN jobs j ON j.id = a.job_id
            WHERE a.candidate_id = candidates.id
            ORDER BY a.updated_at DESC, a.id DESC
            LIMIT 1
        )
        WHERE linked_job_title IS NULL
        "#,
        [],
    );

    let scoring_reset_marker: Option<String> = conn
        .query_row(
            "SELECT value_json FROM app_settings WHERE key = 'scoring_v2_reset_done' LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    if scoring_reset_marker.is_none() {
        let now = Utc::now().to_rfc3339();
        conn.execute("DELETE FROM analysis_results", [])?;
        conn.execute("DELETE FROM screening_results", [])?;
        conn.execute("DELETE FROM job_screening_overrides", [])?;
        conn.execute("DELETE FROM screening_dimensions", [])?;
        conn.execute("DELETE FROM screening_templates", [])?;
        conn.execute(
            r#"
            INSERT INTO app_settings(key, value_json, updated_at)
            VALUES ('scoring_v2_reset_done', 'true', ?1)
            ON CONFLICT(key)
            DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at
            "#,
            [now],
        )?;
    }

    Ok(())
}
