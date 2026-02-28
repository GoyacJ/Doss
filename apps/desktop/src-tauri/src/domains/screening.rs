use chrono::Utc;
use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use tauri::State;
use zip::ZipArchive;

use crate::core::state::AppState;
use crate::core::time::now_iso;
use crate::domains::ai_runtime::trim_resume_excerpt;
use crate::domains::jobs::read_job_by_id;
use crate::infra::audit::write_audit;
use crate::infra::db::open_connection;
use crate::models::interview::{InterviewEvaluationPayload, InterviewQuestion};
use crate::models::job::Job;
use crate::models::screening::{
    CreateScreeningTemplateInput, RunScreeningInput, ScreeningDimension, ScreeningResultRecord,
    ScreeningTemplateRecord, SetJobScreeningTemplateInput, UpdateScreeningTemplateInput,
    UpsertScreeningTemplateInput,
};

pub(crate) fn parse_skills(parsed: &Value) -> Vec<String> {
    parsed
        .get("skills")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(|value| value.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub(crate) fn clamp_score(value: i32) -> i32 {
    value.clamp(0, 100)
}

pub(crate) fn round_one_decimal(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

pub(crate) fn default_screening_dimensions() -> Vec<ScreeningDimension> {
    vec![
        ScreeningDimension {
            key: "goal_orientation".to_string(),
            label: "目标导向".to_string(),
            weight: 30,
        },
        ScreeningDimension {
            key: "team_collaboration".to_string(),
            label: "团队协作".to_string(),
            weight: 15,
        },
        ScreeningDimension {
            key: "self_drive".to_string(),
            label: "自驱力".to_string(),
            weight: 15,
        },
        ScreeningDimension {
            key: "reflection_iteration".to_string(),
            label: "反思迭代".to_string(),
            weight: 10,
        },
        ScreeningDimension {
            key: "openness".to_string(),
            label: "开放性".to_string(),
            weight: 8,
        },
        ScreeningDimension {
            key: "resilience".to_string(),
            label: "抗压韧性".to_string(),
            weight: 7,
        },
        ScreeningDimension {
            key: "learning_ability".to_string(),
            label: "学习能力".to_string(),
            weight: 10,
        },
        ScreeningDimension {
            key: "values_fit".to_string(),
            label: "价值观契合".to_string(),
            weight: 5,
        },
    ]
}

pub(crate) fn normalize_screening_dimensions(
    dimensions: Option<Vec<ScreeningDimension>>,
) -> Result<Vec<ScreeningDimension>, String> {
    let raw = dimensions.unwrap_or_else(default_screening_dimensions);
    if raw.is_empty() {
        return Err("screening_dimensions_empty".to_string());
    }

    let mut seen = BTreeMap::<String, bool>::new();
    let mut normalized = Vec::<ScreeningDimension>::new();
    let mut total_weight = 0_i32;

    for item in raw {
        let key = item.key.trim().to_lowercase();
        let label = item.label.trim().to_string();
        if key.is_empty() || label.is_empty() {
            return Err("screening_dimension_key_or_label_empty".to_string());
        }
        if item.weight <= 0 {
            return Err("screening_dimension_weight_must_be_positive".to_string());
        }
        if seen.insert(key.clone(), true).is_some() {
            return Err(format!("screening_dimension_key_duplicate:{key}"));
        }

        total_weight += item.weight;
        normalized.push(ScreeningDimension {
            key,
            label,
            weight: item.weight,
        });
    }

    if total_weight != 100 {
        return Err(format!(
            "screening_dimension_weight_sum_invalid:{total_weight}"
        ));
    }

    Ok(normalized)
}

pub(crate) fn parse_dimensions_from_config(value: &Value) -> Option<Vec<ScreeningDimension>> {
    let array = value
        .get("dimensions")
        .and_then(|item| item.as_array())
        .cloned()?;
    let mut dimensions = Vec::new();
    for item in array {
        let key = item
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let label = item
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let weight = item.get("weight").and_then(|v| v.as_i64()).unwrap_or(0);
        if key.is_empty() || label.is_empty() || weight <= 0 {
            continue;
        }
        dimensions.push(ScreeningDimension {
            key: key.to_lowercase(),
            label: label.to_string(),
            weight: weight as i32,
        });
    }

    if dimensions.is_empty() {
        None
    } else {
        Some(dimensions)
    }
}

pub(crate) fn load_screening_dimensions(
    conn: &Connection,
    template_id: i64,
) -> Result<Vec<ScreeningDimension>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT dimension_key, dimension_label, weight
            FROM screening_dimensions
            WHERE template_id = ?1
            ORDER BY sort_order ASC
            "#,
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([template_id], |row| {
            Ok(ScreeningDimension {
                key: row.get(0)?,
                label: row.get(1)?,
                weight: row.get(2)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub(crate) fn read_screening_template_by_id(
    conn: &Connection,
    template_id: i64,
) -> Result<ScreeningTemplateRecord, String> {
    let row = conn
        .query_row(
            r#"
            SELECT id, scope, job_id, name, config_json, created_at, updated_at
            FROM screening_templates
            WHERE id = ?1
            "#,
            [template_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .map_err(|error| error.to_string())?;

    let config_json: Value = serde_json::from_str(&row.4).unwrap_or(Value::Null);
    let mut dimensions = load_screening_dimensions(conn, row.0)?;
    if dimensions.is_empty() {
        dimensions =
            parse_dimensions_from_config(&config_json).unwrap_or_else(default_screening_dimensions);
    }
    let risk_rules = config_json
        .get("riskRules")
        .cloned()
        .or_else(|| config_json.get("risk_rules").cloned())
        .unwrap_or_else(|| serde_json::json!({}));

    Ok(ScreeningTemplateRecord {
        id: row.0,
        scope: row.1,
        job_id: row.2,
        name: row.3,
        dimensions,
        risk_rules,
        created_at: row.5,
        updated_at: row.6,
    })
}

pub(crate) fn upsert_screening_template_internal(
    conn: &Connection,
    scope: &str,
    job_id: Option<i64>,
    name: String,
    dimensions: Vec<ScreeningDimension>,
    risk_rules: Value,
) -> Result<ScreeningTemplateRecord, String> {
    let now = now_iso();
    let existing_id = if let Some(job_id_value) = job_id {
        conn.query_row(
            "SELECT id FROM screening_templates WHERE scope = ?1 AND job_id = ?2 LIMIT 1",
            params![scope, job_id_value],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
    } else if scope == "global" {
        resolve_resident_default_global_template_id(conn)?
    } else {
        conn.query_row(
            "SELECT id FROM screening_templates WHERE scope = ?1 AND job_id IS NULL LIMIT 1",
            [scope],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
    };

    let config_json = serde_json::json!({
        "dimensions": dimensions,
        "riskRules": risk_rules,
    })
    .to_string();

    let template_id = if let Some(existing) = existing_id {
        conn.execute(
            r#"
            UPDATE screening_templates
            SET name = ?1, config_json = ?2, updated_at = ?3
            WHERE id = ?4
            "#,
            params![name, config_json, now, existing],
        )
        .map_err(|error| error.to_string())?;
        existing
    } else {
        conn.execute(
            r#"
            INSERT INTO screening_templates(scope, job_id, name, config_json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![scope, job_id, name, config_json, now, now],
        )
        .map_err(|error| error.to_string())?;
        conn.last_insert_rowid()
    };

    conn.execute(
        "DELETE FROM screening_dimensions WHERE template_id = ?1",
        [template_id],
    )
    .map_err(|error| error.to_string())?;

    let final_template = read_screening_template_by_id(conn, template_id)?;
    for (index, dimension) in final_template.dimensions.iter().enumerate() {
        conn.execute(
            r#"
            INSERT INTO screening_dimensions(
                template_id, dimension_key, dimension_label, weight, sort_order, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                template_id,
                dimension.key,
                dimension.label,
                dimension.weight,
                index as i32,
                now,
                now,
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    if scope == "job" {
        if let Some(job_id_value) = job_id {
            conn.execute(
                r#"
                INSERT INTO job_screening_overrides(job_id, template_id, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(job_id)
                DO UPDATE SET template_id = excluded.template_id, updated_at = excluded.updated_at
                "#,
                params![job_id_value, template_id, now, now],
            )
            .map_err(|error| error.to_string())?;
        }
    }

    read_screening_template_by_id(conn, template_id)
}

pub(crate) fn create_global_screening_template_internal(
    conn: &Connection,
    name: String,
    dimensions: Vec<ScreeningDimension>,
    risk_rules: Value,
) -> Result<ScreeningTemplateRecord, String> {
    let now = now_iso();
    let config_json = serde_json::json!({
        "dimensions": dimensions,
        "riskRules": risk_rules,
    })
    .to_string();

    conn.execute(
        r#"
        INSERT INTO screening_templates(scope, job_id, name, config_json, created_at, updated_at)
        VALUES ('global', NULL, ?1, ?2, ?3, ?4)
        "#,
        params![name, config_json, now, now],
    )
    .map_err(|error| error.to_string())?;
    let template_id = conn.last_insert_rowid();

    conn.execute(
        "DELETE FROM screening_dimensions WHERE template_id = ?1",
        [template_id],
    )
    .map_err(|error| error.to_string())?;

    let final_template = read_screening_template_by_id(conn, template_id)?;
    for (index, dimension) in final_template.dimensions.iter().enumerate() {
        conn.execute(
            r#"
            INSERT INTO screening_dimensions(
                template_id, dimension_key, dimension_label, weight, sort_order, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                template_id,
                dimension.key,
                dimension.label,
                dimension.weight,
                index as i32,
                now,
                now,
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    read_screening_template_by_id(conn, template_id)
}

pub(crate) fn update_global_screening_template_internal(
    conn: &Connection,
    template_id: i64,
    name: String,
    dimensions: Vec<ScreeningDimension>,
    risk_rules: Value,
) -> Result<ScreeningTemplateRecord, String> {
    let existing = read_screening_template_by_id(conn, template_id)?;
    if existing.scope != "global" {
        return Err("screening_template_scope_invalid".to_string());
    }

    let now = now_iso();
    let config_json = serde_json::json!({
        "dimensions": dimensions,
        "riskRules": risk_rules,
    })
    .to_string();
    conn.execute(
        r#"
        UPDATE screening_templates
        SET name = ?1, config_json = ?2, updated_at = ?3
        WHERE id = ?4
        "#,
        params![name, config_json, now, template_id],
    )
    .map_err(|error| error.to_string())?;

    conn.execute(
        "DELETE FROM screening_dimensions WHERE template_id = ?1",
        [template_id],
    )
    .map_err(|error| error.to_string())?;

    let final_template = read_screening_template_by_id(conn, template_id)?;
    for (index, dimension) in final_template.dimensions.iter().enumerate() {
        conn.execute(
            r#"
            INSERT INTO screening_dimensions(
                template_id, dimension_key, dimension_label, weight, sort_order, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                template_id,
                dimension.key,
                dimension.label,
                dimension.weight,
                index as i32,
                now,
                now,
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    read_screening_template_by_id(conn, template_id)
}

pub(crate) fn resolve_resident_default_global_template_id(
    conn: &Connection,
) -> Result<Option<i64>, String> {
    let named_default = conn
        .query_row(
            r#"
            SELECT id
            FROM screening_templates
            WHERE scope = 'global' AND name = '默认筛选模板'
            ORDER BY created_at ASC, id ASC
            LIMIT 1
            "#,
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if named_default.is_some() {
        return Ok(named_default);
    }

    conn.query_row(
        r#"
        SELECT id
        FROM screening_templates
        WHERE scope = 'global'
        ORDER BY created_at ASC, id ASC
        LIMIT 1
        "#,
        [],
        |row| row.get::<_, i64>(0),
    )
    .optional()
    .map_err(|error| error.to_string())
}

pub(crate) fn list_global_screening_templates(
    conn: &Connection,
) -> Result<Vec<ScreeningTemplateRecord>, String> {
    let default_template_id = resolve_resident_default_global_template_id(conn)?.unwrap_or(-1);
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id
            FROM screening_templates
            WHERE scope = 'global'
            ORDER BY CASE WHEN id = ?1 THEN 0 ELSE 1 END ASC, updated_at DESC, id DESC
            "#,
        )
        .map_err(|error| error.to_string())?;

    let ids = stmt
        .query_map([default_template_id], |row| row.get::<_, i64>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    let mut templates = Vec::new();
    for id in ids {
        templates.push(read_screening_template_by_id(conn, id)?);
    }
    Ok(templates)
}

pub(crate) fn resolve_screening_template(
    conn: &Connection,
    job_id: Option<i64>,
) -> Result<ScreeningTemplateRecord, String> {
    if let Some(job_id_value) = job_id {
        let template_id = conn
            .query_row(
                r#"
                SELECT st.id
                FROM job_screening_overrides jo
                JOIN screening_templates st ON st.id = jo.template_id
                WHERE jo.job_id = ?1
                LIMIT 1
                "#,
                [job_id_value],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;

        if let Some(id) = template_id {
            return read_screening_template_by_id(conn, id);
        }
    }

    let global_template_id = resolve_resident_default_global_template_id(conn)?;

    if let Some(id) = global_template_id {
        return read_screening_template_by_id(conn, id);
    }

    upsert_screening_template_internal(
        conn,
        "global",
        None,
        "默认筛选模板".to_string(),
        normalize_screening_dimensions(None)?,
        serde_json::json!({}),
    )
}

pub(crate) fn parse_job_required_skills(description: &str) -> Vec<String> {
    description
        .split(|char: char| !char.is_alphanumeric() && char != '+')
        .filter(|token| token.len() >= 3)
        .take(10)
        .map(|token| token.to_lowercase())
        .collect()
}

pub(crate) fn parse_job_salary_max(salary_text: &str) -> Option<f64> {
    salary_text
        .split(|item| item == '-' || item == '~' || item == '到')
        .filter_map(|item| item.trim().parse::<f64>().ok())
        .max_by(|left, right| left.total_cmp(right))
}

pub(crate) fn normalize_interview_questions(
    questions: Vec<InterviewQuestion>,
) -> Result<Vec<InterviewQuestion>, String> {
    if questions.is_empty() {
        return Err("interview_questions_empty".to_string());
    }

    let mut normalized = Vec::new();
    for item in questions {
        let primary_question = item.primary_question.trim().to_string();
        if primary_question.is_empty() {
            return Err("interview_question_primary_empty".to_string());
        }

        let follow_ups = item
            .follow_ups
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        let scoring_points = item
            .scoring_points
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        let red_flags = item
            .red_flags
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();

        if follow_ups.is_empty() {
            return Err("interview_followups_empty".to_string());
        }
        if scoring_points.is_empty() {
            return Err("interview_scoring_points_empty".to_string());
        }

        normalized.push(InterviewQuestion {
            primary_question,
            follow_ups,
            scoring_points,
            red_flags,
        });
    }

    Ok(normalized)
}

pub(crate) fn build_interview_slot_key(candidate_id: i64, job_id: Option<i64>) -> String {
    format!("{candidate_id}:{}", job_id.unwrap_or_default())
}

pub(crate) fn collect_numeric_scores(value: &Value, scores: &mut Vec<f64>) {
    match value {
        Value::Number(number) => {
            if let Some(value) = number.as_f64() {
                scores.push(value);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_numeric_scores(item, scores);
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                collect_numeric_scores(item, scores);
            }
        }
        _ => {}
    }
}

pub(crate) fn collect_string_values(value: &Value) -> Vec<String> {
    match value {
        Value::String(text) => {
            let normalized = text.trim();
            if normalized.is_empty() {
                Vec::new()
            } else {
                vec![normalized.to_string()]
            }
        }
        Value::Array(items) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::trim))
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

pub(crate) fn build_interview_evidence(transcript: &str) -> Vec<String> {
    let mut evidence = transcript
        .lines()
        .map(str::trim)
        .filter(|line| line.chars().count() >= 8)
        .take(3)
        .map(|line| trim_resume_excerpt(line, 120))
        .collect::<Vec<_>>();

    if evidence.is_empty() {
        let fallback = trim_resume_excerpt(transcript.trim(), 120);
        if !fallback.is_empty() {
            evidence.push(fallback);
        }
    }

    evidence
}

pub(crate) fn build_generated_interview_questions(
    role_title: Option<&str>,
    candidate_name: &str,
    years_of_experience: f64,
    required_skills: &[String],
    extracted_skills: &[String],
    screening_recommendation: Option<&str>,
    screening_risk_level: Option<&str>,
    latest_analysis_risks: &[String],
) -> Vec<InterviewQuestion> {
    let role_label = role_title.unwrap_or("目标岗位");
    let normalized_extracted = extracted_skills
        .iter()
        .map(|item| item.to_lowercase())
        .collect::<Vec<_>>();
    let primary_skill = extracted_skills
        .first()
        .cloned()
        .or_else(|| required_skills.first().cloned())
        .unwrap_or_else(|| "岗位核心能力".to_string());
    let missing_skills = required_skills
        .iter()
        .filter(|item| {
            !normalized_extracted
                .iter()
                .any(|owned| owned.contains(&item.to_lowercase()))
        })
        .take(2)
        .cloned()
        .collect::<Vec<_>>();

    let mut questions = vec![
        InterviewQuestion {
            primary_question: format!(
                "请你复盘最近一个最有代表性的项目，重点说明你如何用 {} 在 {} 中达成可量化结果。",
                primary_skill, role_label
            ),
            follow_ups: vec![
                "这个项目你负责的关键决策点是什么？".to_string(),
                "当方案受限时你如何权衡进度、质量和成本？".to_string(),
                "如果重做一次你会优先优化哪一部分？".to_string(),
            ],
            scoring_points: vec![
                "能讲清目标、约束、动作与结果链路".to_string(),
                "有可验证指标（效率、收益、稳定性等）".to_string(),
                "具备复盘与迭代意识".to_string(),
            ],
            red_flags: vec![
                "项目描述停留在职责罗列，无具体结果".to_string(),
                "无法说明个人贡献与团队贡献边界".to_string(),
            ],
        },
        InterviewQuestion {
            primary_question: format!(
                "请描述一次跨团队协作推动复杂事项落地的经历，{} 在其中承担了什么角色？",
                candidate_name
            ),
            follow_ups: vec![
                "冲突或分歧是如何被解决的？".to_string(),
                "你如何管理不同角色的预期？".to_string(),
                "出现延期时你如何对齐优先级？".to_string(),
            ],
            scoring_points: vec![
                "能体现协作、沟通与影响力".to_string(),
                "对风险管理和推进节奏有方法".to_string(),
                "复盘中能体现团队视角".to_string(),
            ],
            red_flags: vec![
                "把问题完全归因于他人".to_string(),
                "回避沟通与责任承担".to_string(),
            ],
        },
    ];

    if !missing_skills.is_empty() {
        questions.push(InterviewQuestion {
            primary_question: format!(
                "JD 中包含 {}，但你简历证据较少。请给出可迁移经验并说明 30 天补齐计划。",
                missing_skills.join(" / ")
            ),
            follow_ups: vec![
                "哪些能力可以迁移，哪些需要补课？".to_string(),
                "你会如何验证补齐后的产出质量？".to_string(),
            ],
            scoring_points: vec![
                "迁移路径清晰且有落地动作".to_string(),
                "能给出可执行学习计划和验证标准".to_string(),
            ],
            red_flags: vec![
                "无法说明补齐方案，仅泛泛而谈".to_string(),
                "对关键能力差距没有风险意识".to_string(),
            ],
        });
    } else {
        questions.push(InterviewQuestion {
            primary_question: format!(
                "请现场拆解一个 {} 场景中的复杂问题，你会如何定义成功标准？",
                role_label
            ),
            follow_ups: vec![
                "第一周和第一个月的推进计划是什么？".to_string(),
                "你会如何设计监控指标和回滚策略？".to_string(),
            ],
            scoring_points: vec![
                "问题拆解完整，优先级清晰".to_string(),
                "有工程落地和风险兜底意识".to_string(),
            ],
            red_flags: vec![
                "只讲理念，不给执行路径".to_string(),
                "忽略风险与兜底机制".to_string(),
            ],
        });
    }

    let risk_topic = if screening_risk_level == Some("HIGH")
        || screening_recommendation == Some("REVIEW")
        || !latest_analysis_risks.is_empty()
    {
        "请针对你过去经历中最可能影响岗位胜任力的风险点做一次主动说明。"
    } else {
        "请举例说明你在高压交付场景下如何维持质量与协作。"
    };
    questions.push(InterviewQuestion {
        primary_question: risk_topic.to_string(),
        follow_ups: vec![
            "你如何识别早期预警信号？".to_string(),
            "若再次发生类似情况你会如何调整？".to_string(),
        ],
        scoring_points: vec![
            "能正视风险，不回避问题".to_string(),
            "提出可执行的预防和修复策略".to_string(),
        ],
        red_flags: vec![
            "把风险解释为“运气不好”且无改进方案".to_string(),
            "对失败复盘缺失".to_string(),
        ],
    });

    questions.push(InterviewQuestion {
        primary_question: format!(
            "结合你 {} 年经验，为什么你认为自己当前阶段适合这个岗位？",
            years_of_experience
        ),
        follow_ups: vec![
            "入职前 90 天你最希望达成的目标是什么？".to_string(),
            "如果实际岗位与预期不一致，你会如何调整？".to_string(),
        ],
        scoring_points: vec![
            "动机真实且与岗位目标一致".to_string(),
            "对业务和个人发展路径有清晰预期".to_string(),
        ],
        red_flags: vec![
            "求职动机仅围绕薪资且回避岗位挑战".to_string(),
            "对岗位内容和业务缺乏理解".to_string(),
        ],
    });

    questions
}

pub(crate) fn evaluate_interview_feedback_payload(
    transcript_text: &str,
    structured_feedback: &Value,
) -> InterviewEvaluationPayload {
    let transcript_len = transcript_text.chars().count();
    let mut raw_scores = Vec::new();
    if let Some(scores) = structured_feedback.get("scores") {
        collect_numeric_scores(scores, &mut raw_scores);
    } else {
        collect_numeric_scores(structured_feedback, &mut raw_scores);
    }

    let normalized_scores = raw_scores
        .into_iter()
        .filter(|value| value.is_finite())
        .map(|value| {
            if value > 0.0 && value <= 1.0 {
                value * 5.0
            } else {
                value
            }
        })
        .map(|value| value.clamp(0.0, 5.0))
        .collect::<Vec<_>>();
    let score_count = normalized_scores.len();
    let score_avg = if score_count == 0 {
        0.0
    } else {
        normalized_scores.iter().sum::<f64>() / score_count as f64
    };

    let transcript_quality = if transcript_len >= 900 {
        92.0
    } else if transcript_len >= 600 {
        84.0
    } else if transcript_len >= 320 {
        74.0
    } else if transcript_len >= 120 {
        64.0
    } else {
        46.0
    };
    let structured_quality = if score_count == 0 {
        48.0
    } else {
        score_avg * 20.0
    };
    let mut overall_score =
        clamp_score((structured_quality * 0.7 + transcript_quality * 0.3).round() as i32);

    let mut evidence = build_interview_evidence(transcript_text);
    if let Some(summary) = structured_feedback
        .get("summary")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        evidence.push(format!("面试官总结: {}", trim_resume_excerpt(summary, 120)));
    }
    let red_flags = structured_feedback
        .get("red_flags")
        .map(collect_string_values)
        .unwrap_or_default();
    if !red_flags.is_empty() {
        evidence.push(format!("红旗信号: {}", red_flags.join("；")));
    }

    let mut verification_points = Vec::<String>::new();
    if transcript_len < 120 {
        verification_points.push("面试转写文本不足，请补充完整问答记录。".to_string());
    }
    if score_count < 3 {
        verification_points.push("结构化评分维度不足，至少补充 3 个维度评分。".to_string());
    }
    if evidence.len() < 2 {
        verification_points.push("可引用证据不足，请补充关键问答片段。".to_string());
    }
    if !red_flags.is_empty() {
        verification_points.push("存在红旗信号，建议安排补充追问并交叉验证。".to_string());
    }

    let evidence_insufficient = transcript_len < 120 || score_count < 3 || evidence.len() < 2;
    if evidence_insufficient {
        overall_score = overall_score.min(65);
        if verification_points.is_empty() {
            verification_points.push("当前证据不足，建议补充二面后再决策。".to_string());
        }
        return InterviewEvaluationPayload {
            recommendation: "HOLD".to_string(),
            overall_score,
            confidence: 0.42,
            evidence,
            verification_points,
            uncertainty: "证据不足，当前结论稳定性较低。".to_string(),
        };
    }

    let recommendation = if overall_score >= 80 && score_avg >= 4.0 && red_flags.is_empty() {
        "HIRE"
    } else if overall_score >= 60 && score_avg >= 3.0 {
        "HOLD"
    } else {
        "NO_HIRE"
    }
    .to_string();

    if verification_points.is_empty() {
        if recommendation == "HIRE" {
            verification_points.push("建议安排业务复核面，确认关键场景匹配度。".to_string());
        } else if recommendation == "HOLD" {
            verification_points.push("建议进行补充面，聚焦风险点做定向验证。".to_string());
        } else {
            verification_points.push("如需复议，需补充与风险点相反的客观证据。".to_string());
        }
    }

    let confidence = (0.52
        + (score_count.min(8) as f64) * 0.04
        + (transcript_len.min(1200) as f64 / 1200.0) * 0.18)
        .clamp(0.45, 0.93);
    let uncertainty = if recommendation == "HIRE" {
        "结论较稳定，但仍需关注业务场景迁移风险。"
    } else if recommendation == "HOLD" {
        "存在可提升空间，建议补充关键证据后复评。"
    } else {
        "当前证据显示匹配度不足，结论偏稳定。"
    }
    .to_string();

    InterviewEvaluationPayload {
        recommendation,
        overall_score,
        confidence,
        evidence,
        verification_points,
        uncertainty,
    }
}

pub(crate) fn dimension_signal_score(key: &str, resume_lower: &str, years: f64) -> f64 {
    let keywords: &[&str] = match key {
        "goal_orientation" => &["目标", "结果", "交付", "增长", "kpi", "指标", "owner"],
        "team_collaboration" => &["协作", "团队", "跨部门", "沟通", "配合"],
        "self_drive" => &["主动", "自驱", "独立", "推进", "负责到底"],
        "reflection_iteration" => &["复盘", "迭代", "优化", "改进", "总结"],
        "openness" => &["开放", "反馈", "接受建议", "新技术", "尝试"],
        "resilience" => &["压力", "抗压", "紧急", "高并发", "故障恢复"],
        "learning_ability" => &["学习", "研究", "调研", "证书", "培训"],
        "values_fit" => &["价值观", "诚信", "责任心", "客户", "长期主义"],
        _ => &["项目", "负责", "协作", "优化"],
    };

    let mut score = 3.0_f64;
    let keyword_hits = keywords
        .iter()
        .filter(|keyword| resume_lower.contains(**keyword))
        .count() as f64;
    score += (keyword_hits.min(4.0)) * 0.3;

    if years >= 5.0 {
        score += 0.3;
    } else if years < 1.5 {
        score -= 0.3;
    }

    if resume_lower.len() < 120 {
        score -= 0.3;
    }

    score.clamp(1.0, 5.0)
}

pub(crate) fn normalize_resume_text(text: &str) -> String {
    text.replace('\u{00a0}', " ")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn decode_xml_entities(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

pub(crate) fn extract_docx_xml_text(xml_bytes: &[u8]) -> Result<String, String> {
    let xml_text = String::from_utf8(xml_bytes.to_vec()).map_err(|error| error.to_string())?;
    let regex = Regex::new(r"(?s)<w:t[^>]*>(.*?)</w:t>").map_err(|error| error.to_string())?;
    let mut parts = Vec::new();
    for capture in regex.captures_iter(&xml_text) {
        if let Some(content) = capture.get(1) {
            let text = decode_xml_entities(content.as_str()).trim().to_string();
            if !text.is_empty() {
                parts.push(text);
            }
        }
    }

    Ok(parts.join(" "))
}

pub(crate) fn extract_text_from_docx_bytes(bytes: &[u8]) -> Result<String, String> {
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|error| error.to_string())?;

    let mut sections = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|error| error.to_string())?;
        let name = file.name().to_string();
        if name == "word/document.xml"
            || name.starts_with("word/header")
            || name.starts_with("word/footer")
        {
            let mut xml = Vec::new();
            file.read_to_end(&mut xml)
                .map_err(|error| error.to_string())?;
            let text = extract_docx_xml_text(&xml)?;
            if !text.trim().is_empty() {
                sections.push(text);
            }
        }
    }

    Ok(normalize_resume_text(&sections.join("\n")))
}

pub(crate) fn extract_text_from_pdf_bytes(bytes: &[u8]) -> Result<String, String> {
    let text = pdf_extract::extract_text_from_mem(bytes).map_err(|error| error.to_string())?;
    Ok(normalize_resume_text(&text))
}

pub(crate) fn extract_text_from_plain_bytes(bytes: &[u8]) -> Result<String, String> {
    let text = String::from_utf8(bytes.to_vec()).map_err(|error| error.to_string())?;
    Ok(normalize_resume_text(&text))
}

pub(crate) fn extract_file_extension(file_name: &str) -> String {
    Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .trim()
        .to_lowercase()
}

pub(crate) fn try_tesseract_ocr(bytes: &[u8], extension: &str) -> Result<String, String> {
    let probe = Command::new("tesseract")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if !matches!(probe, Ok(status) if status.success()) {
        return Err("tesseract_not_available".to_string());
    }

    let token = format!(
        "doss-ocr-{}-{}",
        Utc::now().timestamp_millis(),
        rand::random::<u32>()
    );
    let tmp_dir = std::env::temp_dir();
    let input_path = tmp_dir.join(format!("{token}.{extension}"));
    let output_base = tmp_dir.join(format!("{token}-out"));
    let output_text_path = output_base.with_extension("txt");

    fs::write(&input_path, bytes).map_err(|error| error.to_string())?;

    let status = Command::new("tesseract")
        .arg(&input_path)
        .arg(&output_base)
        .arg("-l")
        .arg("chi_sim+eng")
        .status()
        .map_err(|error| error.to_string())?;

    if !status.success() {
        let fallback_status = Command::new("tesseract")
            .arg(&input_path)
            .arg(&output_base)
            .arg("-l")
            .arg("eng")
            .status()
            .map_err(|error| error.to_string())?;
        if !fallback_status.success() {
            let _ = fs::remove_file(&input_path);
            let _ = fs::remove_file(&output_text_path);
            return Err("tesseract_ocr_failed".to_string());
        }
    }

    let text = fs::read_to_string(&output_text_path).map_err(|error| error.to_string())?;
    let _ = fs::remove_file(&input_path);
    let _ = fs::remove_file(&output_text_path);

    Ok(normalize_resume_text(&text))
}

pub(crate) fn build_structured_resume_fields(raw_text: &str) -> Value {
    let lowered = raw_text.to_lowercase();

    let skill_catalog: &[(&str, &[&str])] = &[
        ("Vue3", &["vue3", "vue.js", "vue"]),
        ("TypeScript", &["typescript", "ts"]),
        ("JavaScript", &["javascript", "js"]),
        ("React", &["react"]),
        ("Node.js", &["node.js", "nodejs", "node"]),
        ("Playwright", &["playwright"]),
        ("SQL", &["sql", "mysql", "postgres", "sqlite"]),
        ("Rust", &["rust"]),
        ("Python", &["python"]),
        ("Java", &["java"]),
        ("Go", &["golang", "go"]),
    ];

    let mut skills = Vec::<String>::new();
    for (label, keywords) in skill_catalog {
        if keywords.iter().any(|keyword| lowered.contains(keyword)) {
            skills.push(label.to_string());
        }
    }

    let years_regex = Regex::new(r"(?i)(\d{1,2}(?:\.\d+)?)\s*年").expect("years regex");
    let years_of_experience = years_regex
        .captures_iter(raw_text)
        .filter_map(|capture| {
            capture
                .get(1)
                .and_then(|value| value.as_str().parse::<f64>().ok())
        })
        .fold(0.0_f64, f64::max);

    let salary_context_regex = Regex::new(
        r"(?i)(?:期望薪资|期望|薪资|薪酬|salary)[^\d]{0,8}(\d{1,3})(?:\s*[-~到]\s*(\d{1,3}))?\s*[kK千]",
    )
    .expect("salary context regex");
    let generic_salary_regex = Regex::new(r"(?i)\b(\d{1,3})\s*[kK千]\b").expect("salary regex");

    let expected_salary_k = salary_context_regex
        .captures(raw_text)
        .and_then(|capture| {
            capture
                .get(2)
                .or_else(|| capture.get(1))
                .and_then(|value| value.as_str().parse::<f64>().ok())
        })
        .or_else(|| {
            generic_salary_regex
                .captures(raw_text)
                .and_then(|capture| capture.get(1))
                .and_then(|value| value.as_str().parse::<f64>().ok())
        });

    let education_level = if raw_text.contains("博士") {
        Some("博士")
    } else if raw_text.contains("硕士") {
        Some("硕士")
    } else if raw_text.contains("本科") {
        Some("本科")
    } else if raw_text.contains("大专") {
        Some("大专")
    } else {
        None
    };

    let school_regex = Regex::new(r"([^\s]{2,16}(大学|学院))").expect("school regex");
    let schools = school_regex
        .captures_iter(raw_text)
        .filter_map(|capture| capture.get(1).map(|value| value.as_str().to_string()))
        .take(3)
        .collect::<Vec<_>>();

    let stability_hints = if years_of_experience > 0.0 {
        if years_of_experience < 2.0 {
            vec!["工作年限较短，建议重点验证稳定性".to_string()]
        } else if years_of_experience >= 5.0 {
            vec!["工作年限较长，可优先评估深度与带人经验".to_string()]
        } else {
            vec!["工作年限中等，建议结合项目复杂度综合判断".to_string()]
        }
    } else {
        Vec::new()
    };

    let project_mentions = raw_text.matches("项目").count() as i64;
    let summary = raw_text.chars().take(220).collect::<String>();

    serde_json::json!({
        "skills": skills,
        "yearsOfExperience": if years_of_experience > 0.0 { Some(years_of_experience) } else { None::<f64> },
        "expectedSalaryK": expected_salary_k,
        "education": {
            "level": education_level,
            "schools": schools,
        },
        "projectMentions": project_mentions,
        "stabilityHints": stability_hints,
        "summary": summary,
    })
}

#[tauri::command]
pub(crate) fn get_screening_template(
    state: State<'_, AppState>,
    job_id: Option<i64>,
) -> Result<ScreeningTemplateRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    resolve_screening_template(&conn, job_id)
}

#[tauri::command]
pub(crate) fn upsert_screening_template(
    state: State<'_, AppState>,
    input: UpsertScreeningTemplateInput,
) -> Result<ScreeningTemplateRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let scope = if input.job_id.is_some() {
        "job"
    } else {
        "global"
    };
    let dimensions = normalize_screening_dimensions(input.dimensions)?;
    let risk_rules = input.risk_rules.unwrap_or_else(|| serde_json::json!({}));
    let name = input
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if let Some(job_id) = input.job_id {
                format!("岗位 {job_id} 微调模板")
            } else {
                "默认筛选模板".to_string()
            }
        });

    let template = upsert_screening_template_internal(
        &conn,
        scope,
        input.job_id,
        name,
        dimensions,
        risk_rules,
    )?;

    write_audit(
        &conn,
        "screening.template.upsert",
        "screening_template",
        Some(template.id.to_string()),
        serde_json::json!({
            "scope": template.scope,
            "jobId": template.job_id,
            "name": template.name,
            "dimensions": template.dimensions,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(template)
}

#[tauri::command]
pub(crate) fn list_screening_templates(
    state: State<'_, AppState>,
) -> Result<Vec<ScreeningTemplateRecord>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut templates = list_global_screening_templates(&conn)?;
    if templates.is_empty() {
        let default_template = upsert_screening_template_internal(
            &conn,
            "global",
            None,
            "默认筛选模板".to_string(),
            normalize_screening_dimensions(None)?,
            serde_json::json!({}),
        )?;
        templates = vec![default_template];
    }
    Ok(templates)
}

#[tauri::command]
pub(crate) fn create_screening_template(
    state: State<'_, AppState>,
    input: CreateScreeningTemplateInput,
) -> Result<ScreeningTemplateRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let dimensions = normalize_screening_dimensions(input.dimensions)?;
    let risk_rules = input.risk_rules.unwrap_or_else(|| serde_json::json!({}));
    let name = input
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "新评分模板".to_string());

    let template = create_global_screening_template_internal(&conn, name, dimensions, risk_rules)?;

    write_audit(
        &conn,
        "screening.template.create",
        "screening_template",
        Some(template.id.to_string()),
        serde_json::json!({
            "scope": template.scope,
            "name": template.name,
            "dimensions": template.dimensions,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(template)
}

#[tauri::command]
pub(crate) fn update_screening_template(
    state: State<'_, AppState>,
    input: UpdateScreeningTemplateInput,
) -> Result<ScreeningTemplateRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let existing = read_screening_template_by_id(&conn, input.template_id)?;
    if existing.scope != "global" {
        return Err("screening_template_scope_invalid".to_string());
    }

    let dimensions =
        normalize_screening_dimensions(input.dimensions.or(Some(existing.dimensions.clone())))?;
    let risk_rules = input.risk_rules.unwrap_or(existing.risk_rules.clone());
    let name = input
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or(existing.name);

    let template = update_global_screening_template_internal(
        &conn,
        input.template_id,
        name,
        dimensions,
        risk_rules,
    )?;

    write_audit(
        &conn,
        "screening.template.update",
        "screening_template",
        Some(template.id.to_string()),
        serde_json::json!({
            "scope": template.scope,
            "name": template.name,
            "dimensions": template.dimensions,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(template)
}

pub(crate) fn delete_global_screening_template_internal(
    conn: &Connection,
    template_id: i64,
) -> Result<Vec<ScreeningTemplateRecord>, String> {
    let existing = read_screening_template_by_id(conn, template_id)?;
    if existing.scope != "global" {
        return Err("screening_template_scope_invalid".to_string());
    }

    let default_template_id = resolve_resident_default_global_template_id(conn)?;
    if default_template_id == Some(template_id) {
        return Err("默认筛选模板不可删除，请改为编辑模板内容".to_string());
    }

    let job_usage_count = count_jobs_using_screening_template(conn, template_id)?;
    if job_usage_count > 0 {
        return Err(format!(
            "该评分模板已被 {job_usage_count} 个职位使用，请先为相关职位切换模板后再删除"
        ));
    }

    conn.execute(
        "DELETE FROM screening_templates WHERE id = ?1",
        [template_id],
    )
    .map_err(|error| error.to_string())?;

    let mut templates = list_global_screening_templates(conn)?;
    if templates.is_empty() {
        let default_template = upsert_screening_template_internal(
            conn,
            "global",
            None,
            "默认筛选模板".to_string(),
            normalize_screening_dimensions(None)?,
            serde_json::json!({}),
        )?;
        templates = vec![default_template];
    }

    Ok(templates)
}

#[tauri::command]
pub(crate) fn delete_screening_template(
    state: State<'_, AppState>,
    template_id: i64,
) -> Result<Vec<ScreeningTemplateRecord>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let existing = read_screening_template_by_id(&conn, template_id)?;
    let templates = delete_global_screening_template_internal(&conn, template_id)?;

    write_audit(
        &conn,
        "screening.template.delete",
        "screening_template",
        Some(template_id.to_string()),
        serde_json::json!({
            "scope": existing.scope,
            "name": existing.name,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(templates)
}

pub(crate) fn count_jobs_using_screening_template(
    conn: &Connection,
    template_id: i64,
) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM job_screening_overrides WHERE template_id = ?1",
        [template_id],
        |row| row.get::<_, i64>(0),
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn set_job_screening_template(
    state: State<'_, AppState>,
    input: SetJobScreeningTemplateInput,
) -> Result<Job, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let job_exists = conn
        .query_row("SELECT id FROM jobs WHERE id = ?1", [input.job_id], |row| {
            row.get::<_, i64>(0)
        })
        .optional()
        .map_err(|error| error.to_string())?;
    if job_exists.is_none() {
        return Err(format!("Job {} not found", input.job_id));
    }

    let now = now_iso();
    if let Some(template_id) = input.template_id {
        let scope = conn
            .query_row(
                "SELECT scope FROM screening_templates WHERE id = ?1",
                [template_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("screening_template_not_found:{template_id}"))?;
        if scope != "global" {
            return Err("screening_template_scope_invalid".to_string());
        }

        conn.execute(
            r#"
            INSERT INTO job_screening_overrides(job_id, template_id, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(job_id)
            DO UPDATE SET template_id = excluded.template_id, updated_at = excluded.updated_at
            "#,
            params![input.job_id, template_id, now, now],
        )
        .map_err(|error| error.to_string())?;
    } else {
        conn.execute(
            "DELETE FROM job_screening_overrides WHERE job_id = ?1",
            [input.job_id],
        )
        .map_err(|error| error.to_string())?;
    }

    conn.execute(
        "UPDATE jobs SET updated_at = ?1 WHERE id = ?2",
        params![now, input.job_id],
    )
    .map_err(|error| error.to_string())?;

    let job = read_job_by_id(&conn, input.job_id)?;
    write_audit(
        &conn,
        "job.template.set",
        "job",
        Some(job.id.to_string()),
        serde_json::json!({
            "templateId": input.template_id,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(job)
}

pub(crate) fn derive_screening_recommendation(
    t0_score: f64,
    overall_score: i32,
    risk_level: &str,
) -> String {
    if t0_score < 3.0 {
        return "REJECT".to_string();
    }
    if overall_score >= 80 && risk_level != "HIGH" {
        return "PASS".to_string();
    }
    if overall_score >= 65 || risk_level != "LOW" {
        return "REVIEW".to_string();
    }
    "REJECT".to_string()
}

pub(crate) fn normalize_final_decision(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_uppercase();
    if normalized.is_empty() {
        return Err("final_decision_required".to_string());
    }
    match normalized.as_str() {
        "HIRE" | "OFFERED" => Ok("HIRE".to_string()),
        "NO_HIRE" | "REJECT" | "REJECTED" => Ok("NO_HIRE".to_string()),
        _ => Err("final_decision_invalid".to_string()),
    }
}

pub(crate) fn map_ai_recommendation_to_final_decision(
    recommendation: &str,
) -> Option<&'static str> {
    match recommendation {
        "HIRE" => Some("HIRE"),
        "HOLD" | "NO_HIRE" => Some("NO_HIRE"),
        _ => None,
    }
}

#[tauri::command]
pub(crate) fn run_resume_screening(
    state: State<'_, AppState>,
    input: RunScreeningInput,
) -> Result<ScreeningResultRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;

    let candidate_years = conn
        .query_row(
            "SELECT years_of_experience FROM candidates WHERE id = ?1",
            [input.candidate_id],
            |row| row.get::<_, f64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Candidate {} not found", input.candidate_id))?;

    let (resume_raw_text, resume_parsed): (String, Value) = conn
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
        .ok_or_else(|| "Resume required before screening".to_string())?;

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

    let template = resolve_screening_template(&conn, effective_job_id)?;

    let mut required_skills: Vec<String> = Vec::new();
    let mut max_salary: Option<f64> = None;
    if let Some(job_id) = effective_job_id {
        if let Some((description, salary_k)) = conn
            .query_row(
                "SELECT description, salary_k FROM jobs WHERE id = ?1",
                [job_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| error.to_string())?
        {
            if let Some(description_text) = description {
                required_skills = parse_job_required_skills(&description_text);
            }
            if let Some(salary_text) = salary_k {
                max_salary = parse_job_salary_max(&salary_text);
            }
        }
    }

    let extracted_skills = parse_skills(&resume_parsed);
    let normalized_skills = extracted_skills
        .iter()
        .map(|skill| skill.to_lowercase())
        .collect::<Vec<_>>();
    let matched_skill_count = required_skills
        .iter()
        .filter(|required| {
            normalized_skills
                .iter()
                .any(|owned| owned.contains(*required))
        })
        .count();

    let skill_coverage = if required_skills.is_empty() {
        0.7_f64
    } else {
        matched_skill_count as f64 / required_skills.len() as f64
    };

    let resume_lower = resume_raw_text.to_lowercase();
    let mut t0_score = 3.0_f64;
    if candidate_years >= 5.0 {
        t0_score += 0.8;
    } else if candidate_years >= 3.0 {
        t0_score += 0.4;
    } else if candidate_years < 1.5 {
        t0_score -= 0.8;
    }
    if !required_skills.is_empty() {
        if skill_coverage >= 0.65 {
            t0_score += 0.6;
        } else if skill_coverage < 0.35 {
            t0_score -= 0.8;
        }
    }
    if resume_raw_text.chars().count() < 120 {
        t0_score -= 0.6;
    }
    t0_score = round_one_decimal(t0_score.clamp(1.0, 5.0));

    let mut t1_acc = 0.0_f64;
    let mut t1_items = Vec::<Value>::new();
    for dimension in &template.dimensions {
        let signal = dimension_signal_score(&dimension.key, &resume_lower, candidate_years);
        let weighted = (signal / 5.0) * dimension.weight as f64;
        t1_acc += weighted;
        t1_items.push(serde_json::json!({
            "key": dimension.key,
            "label": dimension.label,
            "weight": dimension.weight,
            "score": clamp_score(((signal / 5.0) * 100.0).round() as i32),
            "reason": format!("{} 维度信号评分 {:.1}/5.0", dimension.label, round_one_decimal(signal)),
        }));
    }
    let t1_score = clamp_score(t1_acc.round() as i32);

    let education_level = resume_parsed
        .get("education")
        .and_then(|value| value.get("level"))
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let education_score = match education_level {
        "博士" => 95,
        "硕士" => 90,
        "本科" => 82,
        "大专" => 68,
        _ => 72,
    };

    let years_baseline = if required_skills.is_empty() {
        3.0_f64
    } else {
        (required_skills.len() as f64 / 2.0).max(2.0)
    };
    let years_match_score = clamp_score((70.0 + (candidate_years - years_baseline) * 12.0) as i32);

    let industry_risk_score = if resume_lower.contains("转行")
        || resume_lower.contains("跨行业")
        || resume_lower.contains("跨领域")
    {
        55
    } else {
        78
    };

    let expected_salary = resume_parsed
        .get("expectedSalaryK")
        .and_then(|value| value.as_f64());
    let salary_match_score = match (expected_salary, max_salary) {
        (Some(expected), Some(max)) if expected > max + 10.0 => 48,
        (Some(expected), Some(max)) if expected > max + 5.0 => 62,
        (Some(expected), Some(max)) if expected > max => 72,
        (Some(_), Some(_)) => 84,
        _ => 75,
    };

    let fine_score = clamp_score(
        ((education_score + years_match_score + industry_risk_score + salary_match_score) as f64
            / 4.0)
            .round() as i32,
    );

    let project_mentions = resume_parsed
        .get("projectMentions")
        .and_then(|value| value.as_i64())
        .unwrap_or(0);

    let mut bonus_score = 0_i32;
    if project_mentions >= 3 {
        bonus_score += 4;
    } else if project_mentions >= 1 {
        bonus_score += 2;
    }
    if !required_skills.is_empty() && matched_skill_count == required_skills.len() {
        bonus_score += 4;
    }
    if normalized_skills
        .iter()
        .any(|skill| skill.contains("playwright") || skill.contains("rust") || skill.contains("go"))
    {
        bonus_score += 3;
    }
    bonus_score = bonus_score.clamp(0, 15);

    let mut risk_penalty = 0_i32;
    let mut evidence = vec![
        format!("模板: {}", template.name),
        format!(
            "技能匹配: {}/{}",
            matched_skill_count,
            required_skills.len()
        ),
        format!("工作年限: {:.1} 年", candidate_years),
    ];
    let mut verification_points = Vec::<String>::new();

    if t0_score < 3.0 {
        risk_penalty += 12;
        verification_points.push("T0 硬性条件未达标，建议人工二次核验。".to_string());
    }
    if !required_skills.is_empty() && skill_coverage < 0.35 {
        risk_penalty += 10;
        verification_points.push("核心技能覆盖偏低，需在技术面重点核验。".to_string());
    }
    if resume_raw_text.chars().count() < 120 {
        risk_penalty += 8;
        verification_points.push("简历信息较少，建议补充项目证据。".to_string());
    }
    if let (Some(expected), Some(max)) = (expected_salary, max_salary) {
        if expected > max + 8.0 {
            risk_penalty += 10;
            verification_points.push("薪资预期显著高于岗位预算，需先沟通薪资边界。".to_string());
        }
    }

    let risk_level = if risk_penalty >= 18 {
        "HIGH"
    } else if risk_penalty >= 8 {
        "MEDIUM"
    } else {
        "LOW"
    }
    .to_string();

    let overall_score = clamp_score(
        (t1_score as f64 * 0.65 + fine_score as f64 * 0.35 + bonus_score as f64
            - risk_penalty as f64 * 0.8)
            .round() as i32,
    );

    let recommendation = derive_screening_recommendation(t0_score, overall_score, &risk_level);

    if verification_points.is_empty() {
        verification_points.push("可进入面试验证岗位关键能力与价值观匹配度。".to_string());
    }
    evidence.push(format!(
        "综合得分: {} (T1={}, 精筛={}, 加分={}, 风险扣减={})",
        overall_score, t1_score, fine_score, bonus_score, risk_penalty
    ));

    let structured_result = serde_json::json!({
        "weights": {
            "t0": {
                "score": t0_score,
                "rule": "<3不匹配，3-4建议，>=4匹配",
                "matched": t0_score >= 3.0,
                "details": evidence.clone(),
            },
            "t1": {
                "template": template.name,
                "items": t1_items,
            },
            "t2": {
                "bonus": bonus_score,
                "items": verification_points.clone(),
            }
        },
        "risk_alerts": if risk_penalty > 0 { vec![format!("风险扣减 {}", risk_penalty)] } else { Vec::<String>::new() },
        "overall_score": overall_score,
        "overall_comment": recommendation,
    });

    let created_at = now_iso();
    conn.execute(
        r#"
        INSERT INTO screening_results(
            candidate_id, job_id, template_id, t0_score, t1_score, fine_score,
            bonus_score, risk_penalty, overall_score, recommendation, risk_level,
            evidence_json, verification_points_json, structured_result_json, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
        "#,
        params![
            input.candidate_id,
            effective_job_id,
            Some(template.id),
            t0_score,
            t1_score,
            fine_score,
            bonus_score,
            risk_penalty,
            overall_score,
            recommendation,
            risk_level,
            serde_json::to_string(&evidence).map_err(|error| error.to_string())?,
            serde_json::to_string(&verification_points).map_err(|error| error.to_string())?,
            structured_result.to_string(),
            created_at,
        ],
    )
    .map_err(|error| error.to_string())?;

    let id = conn.last_insert_rowid();
    let result = ScreeningResultRecord {
        id,
        candidate_id: input.candidate_id,
        job_id: effective_job_id,
        template_id: Some(template.id),
        t0_score,
        t1_score,
        fine_score,
        bonus_score,
        risk_penalty,
        overall_score,
        recommendation,
        risk_level,
        evidence,
        verification_points,
        structured_result,
        created_at,
    };

    write_audit(
        &conn,
        "screening.run",
        "screening_result",
        Some(result.id.to_string()),
        serde_json::json!({
            "candidateId": result.candidate_id,
            "jobId": result.job_id,
            "templateId": result.template_id,
            "overallScore": result.overall_score,
            "recommendation": result.recommendation,
            "riskLevel": result.risk_level,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(result)
}

#[tauri::command]
pub(crate) fn list_screening_results(
    state: State<'_, AppState>,
    candidate_id: i64,
) -> Result<Vec<ScreeningResultRecord>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, candidate_id, job_id, template_id, t0_score, t1_score, fine_score,
                   bonus_score, risk_penalty, overall_score, recommendation, risk_level,
                   evidence_json, verification_points_json, structured_result_json, created_at
            FROM screening_results
            WHERE candidate_id = ?1
            ORDER BY created_at DESC
            "#,
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([candidate_id], |row| {
            let evidence_text: String = row.get(12)?;
            let verification_text: String = row.get(13)?;
            let structured_text: String = row.get(14)?;
            Ok(ScreeningResultRecord {
                id: row.get(0)?,
                candidate_id: row.get(1)?,
                job_id: row.get(2)?,
                template_id: row.get(3)?,
                t0_score: row.get(4)?,
                t1_score: row.get(5)?,
                fine_score: row.get(6)?,
                bonus_score: row.get(7)?,
                risk_penalty: row.get(8)?,
                overall_score: row.get(9)?,
                recommendation: row.get(10)?,
                risk_level: row.get(11)?,
                evidence: serde_json::from_str(&evidence_text).unwrap_or_default(),
                verification_points: serde_json::from_str(&verification_text).unwrap_or_default(),
                structured_result: serde_json::from_str(&structured_text)
                    .unwrap_or(Value::Object(Default::default())),
                created_at: row.get(15)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}
