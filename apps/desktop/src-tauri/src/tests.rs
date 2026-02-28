use rusqlite::{params, Connection};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::bootstrap::{generate_system_local_key, normalize_local_key};
use crate::core::cipher::FieldCipher;
use crate::domains::ai_runtime::{
    extract_json_object_block, normalize_task_runtime_settings, parse_ai_provider_response,
    parse_minimax_content,
};
use crate::domains::candidate::build_order_by_from_rules;
use crate::domains::jobs::count_active_crawl_tasks_for_job;
use crate::domains::screening::{
    build_structured_resume_fields, count_jobs_using_screening_template,
    create_global_screening_template_internal, delete_global_screening_template_internal,
    derive_screening_recommendation, evaluate_interview_feedback_payload, extract_docx_xml_text,
    normalize_screening_dimensions, resolve_screening_template,
};
use crate::domains::search::build_fts_match_query;
use crate::domains::sidecar_runtime::{sidecar_base_url, sidecar_port_candidates};
use crate::infra::db::migrate_db;
use crate::models::ai::TaskRuntimeSettings;
use crate::models::candidate::SortRule;
use crate::models::common::{is_valid_transition, resolve_qualification_stage, AiProvider};
use crate::models::screening::ScreeningDimension;

#[test]
fn pipeline_transition_rules_are_enforced() {
    assert!(is_valid_transition("NEW", "SCREENING"));
    assert!(!is_valid_transition("NEW", "OFFERED"));
}

#[test]
fn candidate_qualification_stage_resolution_is_deterministic() {
    assert_eq!(
        resolve_qualification_stage("SCREENING", false),
        Some("REJECTED")
    );
    assert_eq!(resolve_qualification_stage("REJECTED", false), None);
    assert_eq!(resolve_qualification_stage("REJECTED", true), Some("NEW"));
    assert_eq!(resolve_qualification_stage("INTERVIEW", true), None);
}

#[test]
fn field_cipher_roundtrip_works() {
    let cipher = FieldCipher::from_seed("unit-test-seed");
    let encrypted = cipher.encrypt("13800000000").expect("encrypt");
    let decrypted = cipher.decrypt(&encrypted).expect("decrypt");
    assert_eq!(decrypted, "13800000000");
}

#[test]
fn extract_json_object_block_works_for_markdown_wrapped_json() {
    let text = "模型输出如下:\n```json\n{\"overall_score\":88,\"dimension_scores\":[]}\n```";
    let extracted = extract_json_object_block(text).expect("extract json");
    assert_eq!(extracted, "{\"overall_score\":88,\"dimension_scores\":[]}");
}

#[test]
fn parse_ai_provider_response_accepts_camel_case_keys() {
    let text = r#"{
      "overallScore": 91,
      "dimensionScores": [
        { "key": "skill_match", "score": 90, "reason": "技能匹配好" },
        { "key": "experience", "score": 88, "reason": "年限满足" },
        { "key": "compensation", "score": 85, "reason": "预算匹配" },
        { "key": "stability", "score": 84, "reason": "稳定性正常" }
      ],
      "risks": ["需确认业务领域经验"],
      "highlights": ["核心技能覆盖充分"],
      "suggestions": ["安排技术面"],
      "evidence": [
        {
          "dimension": "skill_match",
          "statement": "命中 Vue3 / TypeScript",
          "sourceSnippet": "候选人技能: Vue3, TypeScript"
        }
      ],
      "confidence": 0.86
    }"#;

    let parsed = parse_ai_provider_response(text).expect("parse provider response");
    assert_eq!(parsed.overall_score, 91);
    assert_eq!(parsed.dimension_scores.len(), 4);
    assert_eq!(parsed.evidence.len(), 1);
    assert_eq!(parsed.confidence, Some(0.86));
}

#[test]
fn sidecar_port_candidates_include_preferred_and_fallback() {
    let ports = sidecar_port_candidates(3791);
    assert_eq!(ports[0], 3791);
    assert_eq!(ports.len(), 6);
    assert!(ports.contains(&3792));
    assert!(ports.contains(&3796));
}

#[test]
fn sidecar_base_url_uses_localhost() {
    assert_eq!(sidecar_base_url(3791), "http://127.0.0.1:3791");
}

#[test]
fn normalize_local_key_uses_trimmed_env_value() {
    assert_eq!(
        normalize_local_key(Some(" test-secret ".to_string())),
        Some("test-secret".to_string())
    );
    assert_eq!(normalize_local_key(Some("   ".to_string())), None);
    assert_eq!(normalize_local_key(None), None);
}

#[test]
fn generate_system_local_key_returns_random_non_empty_value() {
    let first = generate_system_local_key();
    let second = generate_system_local_key();

    assert!(!first.trim().is_empty());
    assert!(!second.trim().is_empty());
    assert!(first.len() >= 40);
    assert!(second.len() >= 40);
    assert_ne!(first, second);
}

#[test]
fn build_fts_match_query_sanitizes_special_characters() {
    assert_eq!(build_fts_match_query("\""), None);
    assert_eq!(
        build_fts_match_query("Vue3 (TypeScript)"),
        Some("\"vue3\"* AND \"typescript\"*".to_string())
    );
    assert_eq!(
        build_fts_match_query("前端 \"工程师\""),
        Some("\"前端\"* AND \"工程师\"*".to_string())
    );
}

#[test]
fn order_by_builder_uses_whitelist_and_nulls_last() {
    let rules = vec![
        SortRule {
            field: "job_title".to_string(),
            direction: "asc".to_string(),
        },
        SortRule {
            field: "score".to_string(),
            direction: "desc".to_string(),
        },
    ];
    let allowed = [
        ("job_title", "linked_job_title"),
        ("score", "score"),
        ("updated_at", "updated_at"),
    ];

    let order_by = build_order_by_from_rules(
        Some(&rules),
        &allowed,
        "updated_at DESC, id DESC",
    );

    assert_eq!(
        order_by,
        "linked_job_title IS NULL ASC, linked_job_title ASC, score IS NULL ASC, score DESC, id DESC"
    );
}

#[test]
fn order_by_builder_falls_back_when_rules_invalid() {
    let rules = vec![SortRule {
        field: "nonexistent".to_string(),
        direction: "asc".to_string(),
    }];
    let allowed = [("updated_at", "updated_at")];

    let order_by = build_order_by_from_rules(
        Some(&rules),
        &allowed,
        "updated_at DESC, id DESC",
    );

    assert_eq!(order_by, "updated_at DESC, id DESC");
}

#[test]
fn extract_docx_xml_text_collects_runs() {
    let xml = r#"<w:document><w:body><w:p><w:r><w:t>张三</w:t></w:r><w:r><w:t> 5年Vue开发经验 </w:t></w:r></w:p></w:body></w:document>"#;
    let text = extract_docx_xml_text(xml.as_bytes()).expect("extract docx xml");
    assert!(text.contains("张三"));
    assert!(text.contains("5年Vue开发经验"));
}

#[test]
fn build_structured_resume_fields_extracts_skills_and_salary() {
    let raw_text = "候选人熟悉 Vue3 / TypeScript / Playwright，8年经验，期望薪资 45k";
    let parsed: serde_json::Value = build_structured_resume_fields(raw_text);

    let skills = parsed
        .get("skills")
        .and_then(|value| value.as_array())
        .expect("skills");
    assert!(skills.iter().any(|value| value.as_str() == Some("Vue3")));
    assert!(skills
        .iter()
        .any(|value| value.as_str() == Some("TypeScript")));

    let expected_salary = parsed
        .get("expectedSalaryK")
        .and_then(|value| value.as_f64())
        .expect("expectedSalaryK");
    assert_eq!(expected_salary, 45.0);
}

#[test]
fn normalize_task_runtime_settings_clamps_values() {
    let normalized = normalize_task_runtime_settings(TaskRuntimeSettings {
        auto_batch_concurrency: 99,
        auto_retry_count: -10,
        auto_retry_backoff_ms: 20,
    });
    assert_eq!(normalized.auto_batch_concurrency, 8);
    assert_eq!(normalized.auto_retry_count, 0);
    assert_eq!(normalized.auto_retry_backoff_ms, 100);
}

#[test]
fn normalize_screening_dimensions_requires_weight_sum_100() {
    let result = normalize_screening_dimensions(Some(vec![
        ScreeningDimension {
            key: "a".to_string(),
            label: "A".to_string(),
            weight: 60,
        },
        ScreeningDimension {
            key: "b".to_string(),
            label: "B".to_string(),
            weight: 20,
        },
    ]));

    assert!(result.is_err());
}

#[test]
fn normalize_screening_dimensions_returns_default_when_empty_input() {
    let dimensions = normalize_screening_dimensions(None).expect("default dimensions");
    let total = dimensions.iter().map(|item| item.weight).sum::<i32>();
    assert_eq!(total, 100);
    assert!(dimensions.len() >= 4);
}

#[test]
fn screening_recommendation_respects_t0_boundaries() {
    assert_eq!(derive_screening_recommendation(2.9, 88, "LOW"), "REJECT");
    assert_eq!(derive_screening_recommendation(3.0, 82, "LOW"), "PASS");
    assert_eq!(derive_screening_recommendation(3.9, 70, "LOW"), "REVIEW");
    assert_eq!(derive_screening_recommendation(4.0, 60, "LOW"), "REJECT");
}

#[test]
fn interview_evaluation_returns_hold_when_evidence_insufficient() {
    let payload = evaluate_interview_feedback_payload(
        "候选人简单介绍，暂无详细问答。",
        &serde_json::json!({
            "scores": {
                "communication": 3.5
            },
            "summary": "仅完成了短时沟通"
        }),
    );

    assert_eq!(payload.recommendation, "HOLD");
    assert!(payload
        .verification_points
        .iter()
        .any(|item| item.contains("补充")));
}

#[test]
fn ai_provider_catalog_contains_official_providers() {
    let providers = AiProvider::all()
        .iter()
        .map(AiProvider::to_catalog_item)
        .collect::<Vec<_>>();
    let ids = providers
        .iter()
        .map(|item| item.id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        ids,
        vec!["qwen", "doubao", "deepseek", "minimax", "glm", "openapi"]
    );
    assert!(providers
        .iter()
        .all(|item| !item.default_model.trim().is_empty()
            && !item.default_base_url.trim().is_empty()
            && !item.models.is_empty()
            && !item.docs.is_empty()));
}

#[test]
fn parse_minimax_content_handles_openai_style_choices() {
    let body = serde_json::json!({
        "base_resp": {"status_code": 0, "status_msg": "success"},
        "choices": [
            {"message": {"role": "assistant", "content": "OK"}}
        ]
    });

    let parsed = parse_minimax_content(&body).expect("parse minimax content");
    assert_eq!(parsed, "OK");
}

#[test]
fn parse_minimax_content_surfaces_business_error() {
    let body = serde_json::json!({
        "base_resp": {"status_code": 1004, "status_msg": "login fail"}
    });

    let error = parse_minimax_content(&body).expect_err("should fail");
    assert_eq!(error, "provider_api_error_1004: login fail");
}

#[test]
fn ai_provider_from_db_migrates_legacy_mock() {
    assert_eq!(AiProvider::from_db("mock"), AiProvider::Qwen);
    assert_eq!(
        AiProvider::from_db("openai-compatible"),
        AiProvider::OpenApi
    );
    assert_eq!(
        AiProvider::from_db("openai_compatible"),
        AiProvider::OpenApi
    );
    assert_eq!(
        AiProvider::from_db("openapi-compatible"),
        AiProvider::OpenApi
    );
}

#[test]
fn count_jobs_using_screening_template_returns_usage_count() {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    conn.execute(
        "CREATE TABLE job_screening_overrides (
            job_id INTEGER PRIMARY KEY,
            template_id INTEGER NOT NULL
        )",
        [],
    )
    .expect("create overrides table");
    conn.execute(
        "INSERT INTO job_screening_overrides(job_id, template_id) VALUES (?1, ?2)",
        params![101, 9],
    )
    .expect("insert override");
    conn.execute(
        "INSERT INTO job_screening_overrides(job_id, template_id) VALUES (?1, ?2)",
        params![102, 9],
    )
    .expect("insert override");
    conn.execute(
        "INSERT INTO job_screening_overrides(job_id, template_id) VALUES (?1, ?2)",
        params![103, 3],
    )
    .expect("insert override");

    let count = count_jobs_using_screening_template(&conn, 9).expect("count usage");
    assert_eq!(count, 2);
}

fn create_screening_template_schema(conn: &Connection) {
    conn.execute_batch(
        r#"
        CREATE TABLE screening_templates (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            scope TEXT NOT NULL,
            job_id INTEGER,
            name TEXT NOT NULL,
            config_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE screening_dimensions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            template_id INTEGER NOT NULL,
            dimension_key TEXT NOT NULL,
            dimension_label TEXT NOT NULL,
            weight INTEGER NOT NULL,
            sort_order INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE job_screening_overrides (
            job_id INTEGER PRIMARY KEY,
            template_id INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        "#,
    )
    .expect("create screening template schema");
}

#[test]
fn resolve_screening_template_returns_resident_default_when_no_override() {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    create_screening_template_schema(&conn);

    let default_template = create_global_screening_template_internal(
        &conn,
        "默认筛选模板".to_string(),
        normalize_screening_dimensions(None).expect("default dimensions"),
        serde_json::json!({}),
    )
    .expect("create default template");
    let custom_template = create_global_screening_template_internal(
        &conn,
        "前端模板".to_string(),
        normalize_screening_dimensions(None).expect("default dimensions"),
        serde_json::json!({}),
    )
    .expect("create custom template");
    assert_ne!(default_template.id, custom_template.id);

    let resolved = resolve_screening_template(&conn, None).expect("resolve default template");
    assert_eq!(resolved.id, default_template.id);
}

#[test]
fn delete_screening_template_rejects_resident_default_template() {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    create_screening_template_schema(&conn);

    let default_template = create_global_screening_template_internal(
        &conn,
        "默认筛选模板".to_string(),
        normalize_screening_dimensions(None).expect("default dimensions"),
        serde_json::json!({}),
    )
    .expect("create default template");
    let _ = create_global_screening_template_internal(
        &conn,
        "前端模板".to_string(),
        normalize_screening_dimensions(None).expect("default dimensions"),
        serde_json::json!({}),
    )
    .expect("create custom template");

    let result = delete_global_screening_template_internal(&conn, default_template.id);
    assert_eq!(
        result.expect_err("default template should not be deletable"),
        "默认筛选模板不可删除，请改为编辑模板内容".to_string()
    );
}

#[test]
fn count_active_tasks_for_job_counts_pending_running_and_paused() {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    conn.execute(
        "CREATE TABLE crawl_tasks (
            id INTEGER PRIMARY KEY,
            status TEXT NOT NULL,
            payload_json TEXT NOT NULL
        )",
        [],
    )
    .expect("create crawl_tasks table");
    conn.execute(
        "INSERT INTO crawl_tasks(id, status, payload_json) VALUES (?1, ?2, ?3)",
        params![1, "RUNNING", r#"{"localJobId": 101}"#],
    )
    .expect("insert running task");
    conn.execute(
        "INSERT INTO crawl_tasks(id, status, payload_json) VALUES (?1, ?2, ?3)",
        params![2, "PENDING", r#"{"localJobId": 101}"#],
    )
    .expect("insert pending task");
    conn.execute(
        "INSERT INTO crawl_tasks(id, status, payload_json) VALUES (?1, ?2, ?3)",
        params![3, "RUNNING", r#"{"localJobId": 102}"#],
    )
    .expect("insert another job task");
    conn.execute(
        "INSERT INTO crawl_tasks(id, status, payload_json) VALUES (?1, ?2, ?3)",
        params![4, "RUNNING", r#"{}"#],
    )
    .expect("insert no-job task");
    conn.execute(
        "INSERT INTO crawl_tasks(id, status, payload_json) VALUES (?1, ?2, ?3)",
        params![5, "PAUSED", r#"{"localJobId": 101}"#],
    )
    .expect("insert paused task");
    conn.execute(
        "INSERT INTO crawl_tasks(id, status, payload_json) VALUES (?1, ?2, ?3)",
        params![6, "CANCELED", r#"{"localJobId": 101}"#],
    )
    .expect("insert canceled task");

    let count = count_active_crawl_tasks_for_job(&conn, 101).expect("count active tasks");
    assert_eq!(count, 3);
}

#[test]
fn migrate_db_applies_refactor_schema_extensions() {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration since epoch")
        .as_millis();
    let db_path = std::env::temp_dir().join(format!("doss-refactor-schema-{millis}.sqlite3"));

    migrate_db(&db_path).expect("migrate db");
    let conn = Connection::open(&db_path).expect("open db");

    let pending_table_exists: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM sqlite_master WHERE type = 'table' AND name = 'pending_candidates'",
            [],
            |row| row.get(0),
        )
        .expect("check pending_candidates table");
    assert_eq!(pending_table_exists, 1);

    let candidate_has_address: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM pragma_table_info('candidates') WHERE name = 'address'",
            [],
            |row| row.get(0),
        )
        .expect("check candidates.address");
    assert_eq!(candidate_has_address, 1);

    let screening_has_structured_result: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM pragma_table_info('screening_results') WHERE name = 'structured_result_json'",
            [],
            |row| row.get(0),
        )
        .expect("check screening_results.structured_result_json");
    assert_eq!(screening_has_structured_result, 1);

    let crawl_has_schedule_fields: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM pragma_table_info('crawl_tasks') WHERE name IN ('schedule_type', 'schedule_time', 'schedule_day', 'next_run_at')",
            [],
            |row| row.get(0),
        )
        .expect("check crawl task schedule fields");
    assert_eq!(crawl_has_schedule_fields, 4);

    let pending_index_exists: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM sqlite_master WHERE type = 'index' AND name = 'idx_pending_candidates_dedupe'",
            [],
            |row| row.get(0),
        )
        .expect("check pending dedupe index");
    assert_eq!(pending_index_exists, 1);

    let schedule_index_exists: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM sqlite_master WHERE type = 'index' AND name = 'idx_crawl_tasks_next_run_at'",
            [],
            |row| row.get(0),
        )
        .expect("check next_run_at index");
    assert_eq!(schedule_index_exists, 1);

    let _ = std::fs::remove_file(db_path);
}

#[test]
fn migrate_db_handles_legacy_crawl_tasks_without_schedule_columns() {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration since epoch")
        .as_millis();
    let db_path = std::env::temp_dir().join(format!("doss-legacy-migrate-{millis}.sqlite3"));
    let conn = Connection::open(&db_path).expect("open legacy db");

    conn.execute_batch(
        r#"
        CREATE TABLE crawl_tasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source TEXT NOT NULL,
            mode TEXT NOT NULL,
            task_type TEXT NOT NULL,
            status TEXT NOT NULL,
            retry_count INTEGER NOT NULL DEFAULT 0,
            error_code TEXT,
            payload_json TEXT NOT NULL,
            snapshot_json TEXT,
            started_at TEXT,
            finished_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        "#,
    )
    .expect("create legacy crawl_tasks");
    drop(conn);

    migrate_db(&db_path).expect("migrate legacy db");

    let conn = Connection::open(&db_path).expect("open migrated db");
    let crawl_has_schedule_fields: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM pragma_table_info('crawl_tasks') WHERE name IN ('schedule_type', 'schedule_time', 'schedule_day', 'next_run_at')",
            [],
            |row| row.get(0),
        )
        .expect("check schedule fields");
    assert_eq!(crawl_has_schedule_fields, 4);

    let schedule_index_exists: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM sqlite_master WHERE type = 'index' AND name = 'idx_crawl_tasks_next_run_at'",
            [],
            |row| row.get(0),
        )
        .expect("check next_run_at index");
    assert_eq!(schedule_index_exists, 1);

    let _ = std::fs::remove_file(db_path);
}
