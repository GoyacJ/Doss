use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use tauri::{AppHandle, Emitter, State};

use crate::core::state::AppState;
use crate::core::time::now_iso;
use crate::domains::ai_runtime::{
    invoke_text_generation, model_supports_file_upload_for_attachment, parse_json_from_text,
    resolve_ai_settings,
};
use crate::domains::jobs::read_job_by_id;
use crate::domains::recruiting_utils::{
    clamp_score, dimension_signal_score, parse_job_required_skills, parse_job_salary_max,
    round_one_decimal,
};
use crate::domains::resume_materializer::ensure_resume_materialized;
use crate::domains::resume_parser::{
    expected_salary_k_from_parsed_json, parse_skills_from_parsed_json,
    project_mentions_from_parsed_json,
};
use crate::infra::audit::write_audit;
use crate::infra::db::open_connection;
use crate::models::ai::ResolvedAiProviderSettings;
use crate::models::scoring::{
    CreateScoringTemplateInput, RunCandidateScoringInput, ScoringItemConfig, ScoringResultRecord,
    ScoringSectionConfig, ScoringTemplateConfig, ScoringTemplateRecord, ScoringWeights,
    SetJobScoringTemplateInput, UpdateScoringTemplateInput, UpsertScoringTemplateInput,
};

const SCORING_PROGRESS_EVENT: &str = "candidate-ai-analysis-progress";

#[derive(Debug, Clone)]
struct ScoringProgressUpdate {
    phase: &'static str,
    status: &'static str,
    kind: &'static str,
    message: String,
    meta: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CandidateScoringProgressPayload {
    run_id: String,
    candidate_id: i64,
    phase: String,
    status: String,
    kind: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta: Option<Value>,
    at: String,
}

#[derive(Debug, Clone)]
struct ScoredItem {
    key: String,
    label: String,
    description: String,
    weight: i32,
    score_5: f64,
    reason: String,
    evidence: String,
}

#[derive(Debug, Clone)]
struct SectionAssessment {
    score_5: f64,
    items: Vec<ScoredItem>,
    comment: String,
}

#[derive(Debug, Clone)]
struct CandidateScoringContext {
    candidate_years: f64,
    candidate_stage: String,
    candidate_tags: Vec<String>,
    resume_raw_text: String,
    resume_parsed: Value,
    resume_lower: String,
    required_skills: Vec<String>,
    extracted_skills: Vec<String>,
    normalized_skills: Vec<String>,
    matched_skill_count: usize,
    skill_coverage: f64,
    expected_salary_k: Option<f64>,
    max_salary_k: Option<f64>,
    project_mentions: i64,
}

fn scoring_progress_update(
    phase: &'static str,
    status: &'static str,
    kind: &'static str,
    message: impl Into<String>,
    meta: Option<Value>,
) -> ScoringProgressUpdate {
    ScoringProgressUpdate {
        phase,
        status,
        kind,
        message: message.into(),
        meta,
    }
}

fn to_scoring_progress_payload(
    run_id: &str,
    candidate_id: i64,
    update: ScoringProgressUpdate,
) -> CandidateScoringProgressPayload {
    CandidateScoringProgressPayload {
        run_id: run_id.to_string(),
        candidate_id,
        phase: update.phase.to_string(),
        status: update.status.to_string(),
        kind: update.kind.to_string(),
        message: update.message,
        meta: update.meta,
        at: now_iso(),
    }
}

fn emit_scoring_progress(
    app_handle: &AppHandle,
    run_id: &str,
    candidate_id: i64,
    update: ScoringProgressUpdate,
) {
    let payload = to_scoring_progress_payload(run_id, candidate_id, update);
    let _ = app_handle.emit(SCORING_PROGRESS_EVENT, payload);
}

fn default_t1_items() -> Vec<ScoringItemConfig> {
    vec![
        ScoringItemConfig {
            key: "goal_orientation".to_string(),
            label: "目标导向".to_string(),
            description: "是否有明确目标并形成可交付结果。".to_string(),
            weight: 30,
        },
        ScoringItemConfig {
            key: "team_collaboration".to_string(),
            label: "团队协作".to_string(),
            description: "跨角色协作、沟通与推进效率。".to_string(),
            weight: 15,
        },
        ScoringItemConfig {
            key: "self_drive".to_string(),
            label: "自驱力".to_string(),
            description: "主动承担、持续推进和问题闭环能力。".to_string(),
            weight: 15,
        },
        ScoringItemConfig {
            key: "reflection_iteration".to_string(),
            label: "反思迭代".to_string(),
            description: "复盘意识和迭代改进能力。".to_string(),
            weight: 10,
        },
        ScoringItemConfig {
            key: "openness".to_string(),
            label: "开放性".to_string(),
            description: "对反馈与变化的接受度和执行力。".to_string(),
            weight: 8,
        },
        ScoringItemConfig {
            key: "resilience".to_string(),
            label: "抗压韧性".to_string(),
            description: "复杂场景下的稳定性和恢复能力。".to_string(),
            weight: 7,
        },
        ScoringItemConfig {
            key: "learning_ability".to_string(),
            label: "学习能力".to_string(),
            description: "知识吸收与迁移速度。".to_string(),
            weight: 10,
        },
        ScoringItemConfig {
            key: "values_fit".to_string(),
            label: "价值观契合".to_string(),
            description: "与团队协作价值观一致性。".to_string(),
            weight: 5,
        },
    ]
}

pub(crate) fn default_scoring_template_config() -> ScoringTemplateConfig {
    ScoringTemplateConfig {
        weights: ScoringWeights {
            t0: 50,
            t1: 30,
            t2: 10,
            t3: 10,
        },
        t0: ScoringSectionConfig {
            items: vec![
                ScoringItemConfig {
                    key: "required_skills_match".to_string(),
                    label: "岗位技能匹配".to_string(),
                    description: "岗位描述/技能要求与候选人技能覆盖是否匹配。".to_string(),
                    weight: 50,
                },
                ScoringItemConfig {
                    key: "years_experience_match".to_string(),
                    label: "经验年限匹配".to_string(),
                    description: "候选人年限是否满足岗位复杂度要求。".to_string(),
                    weight: 30,
                },
                ScoringItemConfig {
                    key: "resume_completeness".to_string(),
                    label: "简历信息完整度".to_string(),
                    description: "简历证据是否足以支撑判断。".to_string(),
                    weight: 20,
                },
            ],
        },
        t1: ScoringSectionConfig {
            items: default_t1_items(),
        },
        t2: ScoringSectionConfig {
            items: vec![
                ScoringItemConfig {
                    key: "core_skill_bonus".to_string(),
                    label: "核心技能加分".to_string(),
                    description: "核心技能命中程度是否超出岗位最低要求。".to_string(),
                    weight: 40,
                },
                ScoringItemConfig {
                    key: "project_impact_bonus".to_string(),
                    label: "项目影响力加分".to_string(),
                    description: "项目成果是否有可量化业务影响。".to_string(),
                    weight: 30,
                },
                ScoringItemConfig {
                    key: "rare_stack_bonus".to_string(),
                    label: "稀缺技术栈加分".to_string(),
                    description: "是否具备岗位稀缺/高价值技术栈。".to_string(),
                    weight: 30,
                },
            ],
        },
        t3: ScoringSectionConfig {
            items: vec![
                ScoringItemConfig {
                    key: "salary_risk".to_string(),
                    label: "薪资风险".to_string(),
                    description: "薪资预期与岗位预算差异风险（低风险高分）。".to_string(),
                    weight: 35,
                },
                ScoringItemConfig {
                    key: "stability_risk".to_string(),
                    label: "稳定性风险".to_string(),
                    description: "履历稳定性与持续投入风险（低风险高分）。".to_string(),
                    weight: 35,
                },
                ScoringItemConfig {
                    key: "info_completeness_risk".to_string(),
                    label: "信息缺失风险".to_string(),
                    description: "关键信息缺失带来的决策风险（低风险高分）。".to_string(),
                    weight: 30,
                },
            ],
        },
    }
}

fn normalize_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn normalize_comment(text: &str, fallback: &str) -> String {
    let normalized = normalize_text(text);
    if normalized.is_empty() {
        fallback.to_string()
    } else {
        normalized
    }
}

fn score_band(score_5: f64) -> &'static str {
    if score_5 >= 4.0 {
        "较强"
    } else if score_5 >= 3.0 {
        "中等"
    } else {
        "偏弱"
    }
}

fn skills_preview(ctx: &CandidateScoringContext, limit: usize) -> String {
    ctx.extracted_skills
        .iter()
        .take(limit)
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>()
        .join("/")
}

fn required_skill_match_text(ctx: &CandidateScoringContext) -> String {
    if ctx.required_skills.is_empty() {
        return "未配置岗位技能".to_string();
    }
    format!("{}/{}", ctx.matched_skill_count, ctx.required_skills.len())
}

fn default_item_reason(
    section_key: &str,
    item: &ScoringItemConfig,
    score_5: f64,
    ctx: &CandidateScoringContext,
) -> String {
    let band = score_band(score_5);
    let skills = skills_preview(ctx, 2);
    match section_key {
        "t0" => {
            if item.key.contains("skill") {
                return format!(
                    "核心技能匹配{}，建议核验深度。",
                    required_skill_match_text(ctx)
                );
            }
            if item.key.contains("year") || item.label.contains("年限") {
                return format!("{:.1}年经验，年限匹配{}。", ctx.candidate_years, band);
            }
            if item.key.contains("resume") || item.label.contains("完整") {
                if ctx.project_mentions > 0 {
                    return format!("含{}段项目经历，信息较完整。", ctx.project_mentions);
                }
                return "简历项目信息偏少，需补充。".to_string();
            }
            format!("{}与岗位要求{}。", item.label, band)
        }
        "t1" => {
            if !skills.is_empty() {
                format!("{}见{}经历，表现{}。", item.label, skills, band)
            } else {
                format!("{}证据有限，建议面试核验。", item.label)
            }
        }
        "t2" => {
            if item.key.contains("project") {
                return format!(
                    "含{}段项目，{}潜力{}。",
                    ctx.project_mentions, item.label, band
                );
            }
            if item.key.contains("rare") || item.label.contains("稀缺") {
                if let Some(rare_skill) = ctx
                    .extracted_skills
                    .iter()
                    .find(|skill| {
                        let lower = skill.to_lowercase();
                        lower.contains("rust")
                            || lower.contains("go")
                            || lower.contains("playwright")
                            || lower.contains("k8s")
                    })
                    .map(|skill| skill.trim().to_string())
                {
                    return format!("具备{}等栈，稀缺能力可加分。", rare_skill);
                }
            }
            if item.key.contains("core") || item.key.contains("skill") {
                return format!(
                    "核心技能匹配{}，具备加分潜力。",
                    required_skill_match_text(ctx)
                );
            }
            if !skills.is_empty() {
                return format!("简历体现{}能力，{}。", item.label, band);
            }
            format!("{}表现{}，建议补充案例。", item.label, band)
        }
        "t3" => {
            let risk_text = if score_5 >= 3.5 {
                "可控"
            } else if score_5 >= 2.0 {
                "需关注"
            } else {
                "偏高"
            };
            if item.key.contains("salary") || item.label.contains("薪资") {
                if let (Some(expected), Some(max)) = (ctx.expected_salary_k, ctx.max_salary_k) {
                    return format!("期望{:.0}K/预算{:.0}K，风险{}。", expected, max, risk_text);
                }
                return "薪资信息不足，建议确认期望。".to_string();
            }
            if item.key.contains("stability") || item.label.contains("稳定") {
                return format!(
                    "{:.1}年经验，稳定性风险{}。",
                    ctx.candidate_years, risk_text
                );
            }
            if item.key.contains("info") || item.label.contains("信息") {
                if ctx.resume_raw_text.chars().count() < 220 {
                    return "关键信息偏少，决策风险较高。".to_string();
                }
                return "项目与经历信息较全，风险可控。".to_string();
            }
            format!("{}{}。", item.label, risk_text)
        }
        _ => format!("{}表现{}。", item.label, band),
    }
}

fn default_section_comment(section_key: &str, section_score: f64, items: &[ScoredItem]) -> String {
    let title = section_key.to_uppercase();
    if items.is_empty() {
        return format!("{title}小结：简历证据不足，建议补充后复评。");
    }

    let mut top_item = &items[0];
    let mut low_item = &items[0];
    for item in items.iter().skip(1) {
        if item.score_5 > top_item.score_5 {
            top_item = item;
        }
        if item.score_5 < low_item.score_5 {
            low_item = item;
        }
    }

    let advice = match section_key {
        "t0" => format!("建议优先核验{}相关项目证据。", low_item.label),
        "t1" => format!("建议围绕{}追问具体行为案例。", low_item.label),
        "t2" => format!("建议补充{}的量化成果数据。", low_item.label),
        "t3" => format!("建议重点排查{}的真实风险。", low_item.label),
        _ => format!("建议补充{}的关键证据。", low_item.label),
    };

    format!(
        "{title}小结：整体{}（{:.1}/5），优势在{}（{:.1}），短板在{}（{:.1}）。{}",
        score_band(section_score),
        section_score,
        top_item.label,
        top_item.score_5,
        low_item.label,
        low_item.score_5,
        advice
    )
}

fn build_overall_comment_fallback(
    ctx: &CandidateScoringContext,
    _weights: &ScoringWeights,
    overall_score_5: f64,
    _overall_score_100: i32,
    t0: &SectionAssessment,
    t1: &SectionAssessment,
    t2: &SectionAssessment,
    t3: &SectionAssessment,
    recommendation: &str,
    risk_level: &str,
) -> String {
    let recommendation_text = if recommendation == "PASS" {
        "进入下一轮面试"
    } else if recommendation == "REVIEW" {
        "进入人工复核"
    } else {
        "暂缓推进"
    };

    let risk_text = match risk_level {
        "HIGH" => "高风险",
        "MEDIUM" => "中风险",
        _ => "低风险",
    };

    let weakest_module = [
        ("T0", t0.score_5, "硬性匹配与项目深度"),
        ("T1", t1.score_5, "行为能力与协作案例"),
        ("T2", t2.score_5, "项目影响力与加分项证明"),
        ("T3", t3.score_5, "薪资与稳定性风险信息"),
    ]
    .iter()
    .min_by(|a, b| a.1.total_cmp(&b.1))
    .copied()
    .unwrap_or(("T1", t1.score_5, "行为能力与协作案例"));

    let skills = skills_preview(ctx, 3);
    let skills_text = if skills.is_empty() {
        "未提取到明确技能关键词".to_string()
    } else {
        format!("技能关键词包含{}", skills)
    };

    format!(
        "候选人{:.1}年经验，简历提及{}段项目，{}；核心技能匹配{}。综合评分{:.1}/5（T0 {:.1}、T1 {:.1}、T2 {:.1}、T3 {:.1}），当前{}，建议{}。下一步建议围绕{}补充可量化证据，并在面试中重点核验{}。",
        ctx.candidate_years,
        ctx.project_mentions,
        skills_text,
        required_skill_match_text(ctx),
        overall_score_5,
        t0.score_5,
        t1.score_5,
        t2.score_5,
        t3.score_5,
        risk_text,
        recommendation_text,
        weakest_module.0,
        weakest_module.2
    )
}

fn normalize_item(item: &ScoringItemConfig) -> Result<ScoringItemConfig, String> {
    let key = item.key.trim().to_lowercase();
    let label = item.label.trim().to_string();
    let description = item.description.trim().to_string();
    if key.is_empty() || label.is_empty() {
        return Err("scoring_item_key_or_label_empty".to_string());
    }
    if item.weight <= 0 {
        return Err("scoring_item_weight_invalid".to_string());
    }
    Ok(ScoringItemConfig {
        key,
        label,
        description,
        weight: item.weight,
    })
}

fn normalize_section(
    name: &str,
    section: &ScoringSectionConfig,
) -> Result<ScoringSectionConfig, String> {
    if section.items.is_empty() {
        return Err(format!("scoring_section_empty:{name}"));
    }

    let mut seen = BTreeMap::<String, bool>::new();
    let mut items = Vec::<ScoringItemConfig>::new();
    let mut sum = 0_i32;

    for item in &section.items {
        let normalized = normalize_item(item)?;
        if seen.insert(normalized.key.clone(), true).is_some() {
            return Err(format!("scoring_item_key_duplicate:{}", normalized.key));
        }
        sum += normalized.weight;
        items.push(normalized);
    }

    if sum != 100 {
        return Err(format!("scoring_section_weight_sum_invalid:{name}:{sum}"));
    }

    Ok(ScoringSectionConfig { items })
}

pub(crate) fn normalize_scoring_template_config(
    config: Option<ScoringTemplateConfig>,
) -> Result<ScoringTemplateConfig, String> {
    let base = config.unwrap_or_else(default_scoring_template_config);

    let sum = base.weights.t0 + base.weights.t1 + base.weights.t2 + base.weights.t3;
    if base.weights.t0 <= 0 || base.weights.t1 <= 0 || base.weights.t2 <= 0 || base.weights.t3 <= 0
    {
        return Err("scoring_weights_must_be_positive".to_string());
    }
    if sum != 100 {
        return Err(format!("scoring_weights_sum_invalid:{sum}"));
    }

    Ok(ScoringTemplateConfig {
        weights: base.weights,
        t0: normalize_section("t0", &base.t0)?,
        t1: normalize_section("t1", &base.t1)?,
        t2: normalize_section("t2", &base.t2)?,
        t3: normalize_section("t3", &base.t3)?,
    })
}

fn read_scoring_template_by_id(
    conn: &Connection,
    template_id: i64,
) -> Result<ScoringTemplateRecord, String> {
    let row = conn
        .query_row(
            r#"
            SELECT id, scope, job_id, name, config_json, created_at, updated_at
            FROM scoring_templates
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

    let parsed = serde_json::from_str::<ScoringTemplateConfig>(&row.4).ok();
    let config = normalize_scoring_template_config(parsed)?;

    Ok(ScoringTemplateRecord {
        id: row.0,
        scope: row.1,
        job_id: row.2,
        name: row.3,
        config,
        created_at: row.5,
        updated_at: row.6,
    })
}

fn resolve_resident_default_global_template_id(conn: &Connection) -> Result<Option<i64>, String> {
    let named_default = conn
        .query_row(
            r#"
            SELECT id
            FROM scoring_templates
            WHERE scope = 'global' AND name = '默认评分模板'
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
        FROM scoring_templates
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

fn list_global_scoring_templates(conn: &Connection) -> Result<Vec<ScoringTemplateRecord>, String> {
    let default_template_id = resolve_resident_default_global_template_id(conn)?.unwrap_or(-1);
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id
            FROM scoring_templates
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
        templates.push(read_scoring_template_by_id(conn, id)?);
    }
    Ok(templates)
}

fn upsert_scoring_template_internal(
    conn: &Connection,
    scope: &str,
    job_id: Option<i64>,
    name: String,
    config: ScoringTemplateConfig,
) -> Result<ScoringTemplateRecord, String> {
    let now = now_iso();
    let existing_id = if let Some(job_id_value) = job_id {
        conn.query_row(
            "SELECT id FROM scoring_templates WHERE scope = ?1 AND job_id = ?2 LIMIT 1",
            params![scope, job_id_value],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
    } else if scope == "global" {
        resolve_resident_default_global_template_id(conn)?
    } else {
        conn.query_row(
            "SELECT id FROM scoring_templates WHERE scope = ?1 AND job_id IS NULL LIMIT 1",
            [scope],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
    };

    let config_json = serde_json::to_string(&config).map_err(|error| error.to_string())?;
    let template_id = if let Some(existing) = existing_id {
        conn.execute(
            r#"
            UPDATE scoring_templates
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
            INSERT INTO scoring_templates(scope, job_id, name, config_json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![scope, job_id, name, config_json, now, now],
        )
        .map_err(|error| error.to_string())?;
        conn.last_insert_rowid()
    };

    if scope == "job" {
        if let Some(job_id_value) = job_id {
            conn.execute(
                r#"
                INSERT INTO job_scoring_overrides(job_id, template_id, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(job_id)
                DO UPDATE SET template_id = excluded.template_id, updated_at = excluded.updated_at
                "#,
                params![job_id_value, template_id, now, now],
            )
            .map_err(|error| error.to_string())?;
        }
    }

    read_scoring_template_by_id(conn, template_id)
}

fn create_global_scoring_template_internal(
    conn: &Connection,
    name: String,
    config: ScoringTemplateConfig,
) -> Result<ScoringTemplateRecord, String> {
    let now = now_iso();
    let config_json = serde_json::to_string(&config).map_err(|error| error.to_string())?;

    conn.execute(
        r#"
        INSERT INTO scoring_templates(scope, job_id, name, config_json, created_at, updated_at)
        VALUES ('global', NULL, ?1, ?2, ?3, ?4)
        "#,
        params![name, config_json, now, now],
    )
    .map_err(|error| error.to_string())?;
    let template_id = conn.last_insert_rowid();

    read_scoring_template_by_id(conn, template_id)
}

fn update_global_scoring_template_internal(
    conn: &Connection,
    template_id: i64,
    name: String,
    config: ScoringTemplateConfig,
) -> Result<ScoringTemplateRecord, String> {
    let existing = read_scoring_template_by_id(conn, template_id)?;
    if existing.scope != "global" {
        return Err("scoring_template_scope_invalid".to_string());
    }

    let now = now_iso();
    let config_json = serde_json::to_string(&config).map_err(|error| error.to_string())?;

    conn.execute(
        r#"
        UPDATE scoring_templates
        SET name = ?1, config_json = ?2, updated_at = ?3
        WHERE id = ?4
        "#,
        params![name, config_json, now, template_id],
    )
    .map_err(|error| error.to_string())?;

    read_scoring_template_by_id(conn, template_id)
}

fn resolve_scoring_template(
    conn: &Connection,
    job_id: Option<i64>,
) -> Result<ScoringTemplateRecord, String> {
    if let Some(job_id_value) = job_id {
        let template_id = conn
            .query_row(
                r#"
                SELECT st.id
                FROM job_scoring_overrides jo
                JOIN scoring_templates st ON st.id = jo.template_id
                WHERE jo.job_id = ?1
                LIMIT 1
                "#,
                [job_id_value],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;

        if let Some(id) = template_id {
            return read_scoring_template_by_id(conn, id);
        }
    }

    if let Some(id) = resolve_resident_default_global_template_id(conn)? {
        return read_scoring_template_by_id(conn, id);
    }

    upsert_scoring_template_internal(
        conn,
        "global",
        None,
        "默认评分模板".to_string(),
        normalize_scoring_template_config(None)?,
    )
}

fn count_jobs_using_scoring_template(conn: &Connection, template_id: i64) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM job_scoring_overrides WHERE template_id = ?1",
        [template_id],
        |row| row.get::<_, i64>(0),
    )
    .map_err(|error| error.to_string())
}

fn section_score_5(items: &[ScoredItem]) -> f64 {
    let weighted = items
        .iter()
        .map(|item| item.score_5 * (item.weight as f64 / 100.0))
        .sum::<f64>();
    round_one_decimal(weighted.clamp(0.0, 5.0))
}

fn clamp_score_5(value: f64) -> f64 {
    round_one_decimal(value.clamp(0.0, 5.0))
}

fn recommendation_from_scores(t0_score_5: f64, overall_score_100: i32, risk_level: &str) -> String {
    if t0_score_5 < 3.0 {
        return "REJECT".to_string();
    }
    if overall_score_100 >= 80 && risk_level != "HIGH" {
        return "PASS".to_string();
    }
    if overall_score_100 >= 65 || risk_level != "LOW" {
        return "REVIEW".to_string();
    }
    "REJECT".to_string()
}

fn risk_level_from_t3_score(t3_score_5: f64) -> &'static str {
    if t3_score_5 < 2.0 {
        "HIGH"
    } else if t3_score_5 < 3.5 {
        "MEDIUM"
    } else {
        "LOW"
    }
}

fn fallback_score_for_item(
    section_key: &str,
    item: &ScoringItemConfig,
    ctx: &CandidateScoringContext,
) -> f64 {
    let key = item.key.as_str();
    match section_key {
        "t0" => {
            if key.contains("skill") {
                return clamp_score_5(1.0 + 4.0 * ctx.skill_coverage);
            }
            if key.contains("year") || item.label.contains("年限") {
                return if ctx.candidate_years >= 5.0 {
                    4.5
                } else if ctx.candidate_years >= 3.0 {
                    4.0
                } else if ctx.candidate_years >= 1.5 {
                    3.0
                } else {
                    2.0
                };
            }
            if key.contains("resume") || item.label.contains("完整") {
                let len = ctx.resume_raw_text.chars().count();
                return if len >= 400 {
                    4.5
                } else if len >= 220 {
                    4.0
                } else if len >= 120 {
                    3.2
                } else {
                    1.8
                };
            }
            clamp_score_5(2.8 + ctx.skill_coverage)
        }
        "t1" => {
            let signal = dimension_signal_score(&item.key, &ctx.resume_lower, ctx.candidate_years);
            clamp_score_5(signal)
        }
        "t2" => {
            if key.contains("project") {
                return if ctx.project_mentions >= 3 {
                    4.5
                } else if ctx.project_mentions >= 1 {
                    3.5
                } else {
                    2.0
                };
            }
            if key.contains("rare") || item.label.contains("稀缺") {
                let has_rare = ctx.normalized_skills.iter().any(|skill| {
                    skill.contains("playwright") || skill.contains("rust") || skill.contains("go")
                });
                return if has_rare { 4.4 } else { 2.4 };
            }
            if key.contains("core") || item.label.contains("核心") || key.contains("skill") {
                return if ctx.skill_coverage >= 0.8 {
                    4.5
                } else if ctx.skill_coverage >= 0.5 {
                    3.5
                } else {
                    2.2
                };
            }
            3.0
        }
        "t3" => {
            if key.contains("salary") || item.label.contains("薪资") {
                return match (ctx.expected_salary_k, ctx.max_salary_k) {
                    (Some(expected), Some(max)) if expected > max + 8.0 => 1.5,
                    (Some(expected), Some(max)) if expected > max + 3.0 => 2.5,
                    (Some(_), Some(_)) => 4.3,
                    _ => 3.5,
                };
            }
            if key.contains("stability") || item.label.contains("稳定") {
                return if ctx.candidate_years < 1.5 {
                    2.0
                } else if ctx.candidate_years < 3.0 {
                    3.0
                } else {
                    4.2
                };
            }
            if key.contains("info") || item.label.contains("信息") {
                let len = ctx.resume_raw_text.chars().count();
                return if len < 120 {
                    1.5
                } else if len < 220 {
                    2.8
                } else {
                    4.4
                };
            }
            3.0
        }
        _ => 3.0,
    }
}

fn parse_ai_item_map(items: Option<&Vec<Value>>) -> BTreeMap<String, Value> {
    let mut map = BTreeMap::<String, Value>::new();
    if let Some(values) = items {
        for item in values {
            if let Some(key) = item.get("key").and_then(|value| value.as_str()) {
                map.insert(key.trim().to_lowercase(), item.clone());
            }
        }
    }
    map
}

fn as_f64(value: Option<&Value>) -> Option<f64> {
    value.and_then(|item| item.as_f64().or_else(|| item.as_i64().map(|v| v as f64)))
}

fn as_string(value: Option<&Value>) -> String {
    value
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .unwrap_or_default()
}

fn build_section_assessment(
    section_key: &str,
    section: &ScoringSectionConfig,
    ai_section: Option<&Value>,
    ctx: &CandidateScoringContext,
) -> SectionAssessment {
    let ai_items = ai_section
        .and_then(|value| value.get("items"))
        .and_then(|value| value.as_array())
        .map(|values| values.to_vec());
    let ai_map = parse_ai_item_map(ai_items.as_ref());

    let mut scored_items = Vec::<ScoredItem>::new();
    for item in &section.items {
        let ai_item = ai_map.get(&item.key);
        let fallback_score = fallback_score_for_item(section_key, item, ctx);
        let score_5 = as_f64(ai_item.and_then(|value| value.get("score_5")))
            .map(clamp_score_5)
            .unwrap_or(fallback_score);
        let reason = normalize_comment(
            &as_string(ai_item.and_then(|value| value.get("reason"))),
            &default_item_reason(section_key, item, score_5, ctx),
        );
        let evidence = normalize_comment(
            &as_string(ai_item.and_then(|value| value.get("evidence"))),
            "证据来源：候选人简历与岗位描述。",
        );

        scored_items.push(ScoredItem {
            key: item.key.clone(),
            label: item.label.clone(),
            description: item.description.clone(),
            weight: item.weight,
            score_5,
            reason,
            evidence,
        });
    }

    let section_score = section_score_5(&scored_items);
    let default_comment = default_section_comment(section_key, section_score, &scored_items);
    let section_comment = normalize_comment(
        &as_string(ai_section.and_then(|value| value.get("comment"))),
        &default_comment,
    );

    SectionAssessment {
        score_5: section_score,
        items: scored_items,
        comment: section_comment,
    }
}

fn build_scoring_prompts(
    template: &ScoringTemplateRecord,
    ctx: &CandidateScoringContext,
) -> (String, String) {
    let system_prompt = r#"你是招聘评分助手。请严格输出 JSON（不要 markdown，不要额外文本）。
输出结构:
{
  "t0_assessment": {"items": [{"key": "...", "score_5": 0-5, "reason": "...", "evidence": "..."}], "comment": "..."},
  "t1_assessment": {"items": [{"key": "...", "score_5": 0-5, "reason": "...", "evidence": "..."}], "comment": "..."},
  "t2_assessment": {"items": [{"key": "...", "score_5": 0-5, "reason": "...", "evidence": "..."}], "comment": "..."},
  "t3_assessment": {"items": [{"key": "...", "score_5": 0-5, "reason": "...", "evidence": "..."}], "comment": "..."},
  "overall_comment": "...",
  "risk_level": "LOW|MEDIUM|HIGH",
  "highlights": ["..."],
  "risks": ["..."],
  "suggestions": ["..."]
}
约束:
1) 只根据输入信息打分，不得编造。
2) 每个区块 items 的 key 必须来自模板。
3) score_5 保留 1 位小数。
4) 每个指标 reason 为 30 字以内短评，必须包含“简历证据点+判断结论”，禁止空泛描述。
5) 每个区块 comment 为 300 字以内，必须输出“模块小结”，包含优势、短板和下一步建议。
6) overall_comment 为 500 字以内，必须包含简历整体概览、分数解读、整体评价、录用建议与行动建议；若超出请自我压缩重写后输出。
7) 若证据不足，明确写出“信息不足”及需要补充的材料。
8) 避免套话和重复句式，语言简洁客观。"#;

    let payload = serde_json::json!({
        "template": {
            "name": template.name,
            "weights": template.config.weights,
            "t0": template.config.t0,
            "t1": template.config.t1,
            "t2": template.config.t2,
            "t3": template.config.t3,
        },
        "candidate": {
            "years": ctx.candidate_years,
            "stage": ctx.candidate_stage,
            "tags": ctx.candidate_tags,
            "skills": ctx.extracted_skills,
            "matchedSkillCount": ctx.matched_skill_count,
            "requiredSkillCount": ctx.required_skills.len(),
            "skillCoverage": ctx.skill_coverage,
            "expectedSalaryK": ctx.expected_salary_k,
            "projectMentions": ctx.project_mentions,
        },
        "job": {
            "requiredSkills": ctx.required_skills,
            "maxSalaryK": ctx.max_salary_k,
        },
        "resumeParsed": ctx.resume_parsed,
        "resumeText": ctx.resume_raw_text,
    });

    (system_prompt.to_string(), payload.to_string())
}

fn parse_string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(|item| item.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn split_chunk_by_chars(text: &str, max_chars: usize) -> Vec<String> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::<String>::new();
    let mut current = String::new();
    let mut count = 0_usize;
    for ch in text.chars() {
        current.push(ch);
        count += 1;
        if count >= max_chars {
            chunks.push(current.trim().to_string());
            current.clear();
            count = 0;
        }
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }
    chunks
}

fn collect_resume_chunks(ctx: &CandidateScoringContext) -> Vec<String> {
    let sections = ctx
        .resume_parsed
        .get("sections")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let mut chunks = Vec::<String>::new();

    for section in sections {
        let title = section
            .get("title")
            .and_then(|value| value.as_str())
            .unwrap_or("Section")
            .trim();
        let content = section
            .get("content")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim();
        if content.is_empty() {
            continue;
        }
        for chunk in split_chunk_by_chars(content, 2400) {
            chunks.push(format!("[{title}]\n{chunk}"));
        }
    }

    if chunks.is_empty() {
        chunks = split_chunk_by_chars(&ctx.resume_raw_text, 2400);
    }

    chunks
}

fn invoke_text_generation_map_reduce(
    settings: &ResolvedAiProviderSettings,
    system_prompt: &str,
    user_prompt: &str,
    ctx: &CandidateScoringContext,
) -> Result<String, String> {
    let chunks = collect_resume_chunks(ctx);
    if chunks.is_empty() {
        return invoke_text_generation(settings, system_prompt, user_prompt, None);
    }

    let map_system_prompt = r#"你是招聘信息抽取助手。请仅输出 JSON，不要 markdown。
输出结构:
{
  "facts": ["..."],
  "skills": ["..."],
  "highlights": ["..."],
  "risks": ["..."]
}
要求：只根据输入 chunk 内容总结事实，不得编造。"#;
    let mut mapped_rows = Vec::<Value>::new();
    for (index, chunk) in chunks.iter().enumerate() {
        let map_payload = serde_json::json!({
            "chunkIndex": index + 1,
            "chunkTotal": chunks.len(),
            "requiredSkills": ctx.required_skills,
            "chunkText": chunk,
        });
        let map_text =
            invoke_text_generation(settings, map_system_prompt, &map_payload.to_string(), None)?;
        let map_value = parse_json_from_text(&map_text)?;
        mapped_rows.push(map_value);
    }

    let base_payload = serde_json::from_str::<Value>(user_prompt)
        .unwrap_or_else(|_| serde_json::json!({ "rawUserPrompt": user_prompt }));
    let reduce_payload = serde_json::json!({
        "baseInput": base_payload,
        "chunkFacts": mapped_rows,
    });
    invoke_text_generation(settings, system_prompt, &reduce_payload.to_string(), None)
}

fn run_candidate_ai_analysis_blocking<F>(
    state: &AppState,
    input: RunCandidateScoringInput,
    on_progress: F,
) -> Result<ScoringResultRecord, String>
where
    F: FnMut(ScoringProgressUpdate),
{
    let mut on_progress = on_progress;
    on_progress(scoring_progress_update(
        "prepare",
        "running",
        "start",
        "开始读取候选人与岗位上下文",
        None,
    ));

    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;

    let candidate = conn
        .query_row(
            "SELECT id, years_of_experience, stage, tags_json, linked_job_id FROM candidates WHERE id = ?1",
            [input.candidate_id],
            |row| {
                let tags_json: String = row.get(3)?;
                let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, String>(2)?,
                    tags,
                    row.get::<_, Option<i64>>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Candidate {} not found", input.candidate_id))?;

    let materialized_resume = ensure_resume_materialized(&conn, input.candidate_id)?;
    let resume_raw_text = materialized_resume.raw_text.clone();
    let resume_parsed = materialized_resume.parsed_value.clone();

    let effective_job_id = input.job_id.or(candidate.4);

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

    let template = resolve_scoring_template(&conn, effective_job_id)?;
    on_progress(scoring_progress_update(
        "prepare",
        "running",
        "progress",
        format!("已加载评分模板：{}", template.name),
        Some(serde_json::json!({
            "templateId": template.id,
            "jobId": effective_job_id,
        })),
    ));

    let extracted_skills = parse_skills_from_parsed_json(&resume_parsed);
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

    let project_mentions = project_mentions_from_parsed_json(&resume_parsed)
        .max(resume_raw_text.matches("项目").count() as i64);

    let expected_salary_k = expected_salary_k_from_parsed_json(&resume_parsed);

    let ctx = CandidateScoringContext {
        candidate_years: candidate.1,
        candidate_stage: candidate.2,
        candidate_tags: candidate.3,
        resume_raw_text: resume_raw_text.clone(),
        resume_parsed: resume_parsed.clone(),
        resume_lower: resume_raw_text.to_lowercase(),
        required_skills,
        extracted_skills,
        normalized_skills,
        matched_skill_count,
        skill_coverage,
        expected_salary_k,
        max_salary_k: max_salary,
        project_mentions,
    };

    let ai_settings =
        resolve_ai_settings(&conn, &state.cipher).map_err(|error| error.to_string())?;
    let resume_attachment = materialized_resume.attachment.clone();
    if ai_settings.api_key.is_none() {
        return Err("ai_provider_api_key_missing".to_string());
    }
    on_progress(scoring_progress_update(
        "prepare",
        "running",
        "progress",
        "已准备模板化评分输入，开始调用 AI",
        None,
    ));
    let (system_prompt, user_prompt) = build_scoring_prompts(&template, &ctx);
    let ai_content =
        if model_supports_file_upload_for_attachment(&ai_settings, resume_attachment.as_ref()) {
            match invoke_text_generation(
                &ai_settings,
                &system_prompt,
                &user_prompt,
                resume_attachment.as_ref(),
            ) {
                Ok(content) => content,
                Err(_) => invoke_text_generation_map_reduce(
                    &ai_settings,
                    &system_prompt,
                    &user_prompt,
                    &ctx,
                )?,
            }
        } else {
            invoke_text_generation_map_reduce(&ai_settings, &system_prompt, &user_prompt, &ctx)?
        };
    let ai_value = parse_json_from_text(&ai_content)?;

    on_progress(scoring_progress_update(
        "t0",
        "running",
        "start",
        "正在评估 T0 重要指标",
        None,
    ));
    let t0_assessment = build_section_assessment(
        "t0",
        &template.config.t0,
        ai_value.get("t0_assessment"),
        &ctx,
    );

    on_progress(scoring_progress_update(
        "t1",
        "running",
        "start",
        "正在评估 T1 指标配置",
        None,
    ));
    let t1_assessment = build_section_assessment(
        "t1",
        &template.config.t1,
        ai_value.get("t1_assessment"),
        &ctx,
    );

    on_progress(scoring_progress_update(
        "t2",
        "running",
        "start",
        "正在评估 T2 加分项",
        None,
    ));
    let t2_assessment = build_section_assessment(
        "t2",
        &template.config.t2,
        ai_value.get("t2_assessment"),
        &ctx,
    );

    on_progress(scoring_progress_update(
        "t3",
        "running",
        "start",
        "正在评估 T3 风险项",
        None,
    ));
    let t3_assessment = build_section_assessment(
        "t3",
        &template.config.t3,
        ai_value.get("t3_assessment"),
        &ctx,
    );

    let overall_score_5 = round_one_decimal(
        t0_assessment.score_5 * (template.config.weights.t0 as f64 / 100.0)
            + t1_assessment.score_5 * (template.config.weights.t1 as f64 / 100.0)
            + t2_assessment.score_5 * (template.config.weights.t2 as f64 / 100.0)
            + t3_assessment.score_5 * (template.config.weights.t3 as f64 / 100.0),
    );
    let overall_score = clamp_score((overall_score_5 * 20.0).round() as i32);

    let risk_level = as_string(ai_value.get("risk_level"));
    let normalized_risk_level = match risk_level.as_str() {
        "HIGH" | "MEDIUM" | "LOW" => risk_level,
        _ => risk_level_from_t3_score(t3_assessment.score_5).to_string(),
    };

    let recommendation =
        recommendation_from_scores(t0_assessment.score_5, overall_score, &normalized_risk_level);

    let highlights = parse_string_array(ai_value.get("highlights"));
    let risks = parse_string_array(ai_value.get("risks"));
    let suggestions = parse_string_array(ai_value.get("suggestions"));

    let overall_comment_fallback = build_overall_comment_fallback(
        &ctx,
        &template.config.weights,
        overall_score_5,
        overall_score,
        &t0_assessment,
        &t1_assessment,
        &t2_assessment,
        &t3_assessment,
        &recommendation,
        &normalized_risk_level,
    );

    let overall_comment = normalize_comment(
        &as_string(ai_value.get("overall_comment")),
        &overall_comment_fallback,
    );

    let structured_result = serde_json::json!({
        "version": 3,
        "summary": {
            "overall_score_5": overall_score_5,
            "overall_score_100": overall_score,
            "weights": {
                "t0": template.config.weights.t0,
                "t1": template.config.weights.t1,
                "t2": template.config.weights.t2,
                "t3": template.config.weights.t3,
            },
            "subscores": {
                "t0": t0_assessment.score_5,
                "t1": t1_assessment.score_5,
                "t2": t2_assessment.score_5,
                "t3": t3_assessment.score_5,
            },
            "overall_comment": overall_comment,
            "recommendation": recommendation,
            "risk_level": normalized_risk_level,
        },
        "template_assessment": {
            "template": template.name,
            "t0": {
                "score_5": t0_assessment.score_5,
                "comment": t0_assessment.comment,
                "items": t0_assessment.items.iter().map(|item| serde_json::json!({
                    "key": item.key,
                    "label": item.label,
                    "description": item.description,
                    "weight": item.weight,
                    "score_5": item.score_5,
                    "reason": item.reason,
                    "evidence": item.evidence,
                })).collect::<Vec<_>>(),
            },
            "t1": {
                "score_5": t1_assessment.score_5,
                "comment": t1_assessment.comment,
                "items": t1_assessment.items.iter().map(|item| serde_json::json!({
                    "key": item.key,
                    "label": item.label,
                    "description": item.description,
                    "weight": item.weight,
                    "score_5": item.score_5,
                    "reason": item.reason,
                    "evidence": item.evidence,
                })).collect::<Vec<_>>(),
            },
            "t2": {
                "score_5": t2_assessment.score_5,
                "comment": t2_assessment.comment,
                "items": t2_assessment.items.iter().map(|item| serde_json::json!({
                    "key": item.key,
                    "label": item.label,
                    "description": item.description,
                    "weight": item.weight,
                    "score_5": item.score_5,
                    "reason": item.reason,
                    "evidence": item.evidence,
                })).collect::<Vec<_>>(),
            },
            "t3": {
                "score_5": t3_assessment.score_5,
                "comment": t3_assessment.comment,
                "items": t3_assessment.items.iter().map(|item| serde_json::json!({
                    "key": item.key,
                    "label": item.label,
                    "description": item.description,
                    "weight": item.weight,
                    "score_5": item.score_5,
                    "reason": item.reason,
                    "evidence": item.evidence,
                })).collect::<Vec<_>>(),
            },
        },
        "highlights": highlights,
        "risks": risks,
        "suggestions": suggestions,
    });

    on_progress(scoring_progress_update(
        "persist",
        "running",
        "start",
        "正在写入评分结果",
        None,
    ));
    let created_at = now_iso();

    conn.execute(
        r#"
        INSERT INTO scoring_results(
            candidate_id, job_id, template_id,
            overall_score, overall_score_5,
            t0_score_5, t1_score_5, t2_score_5, t3_score_5,
            recommendation, risk_level,
            structured_result_json, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            input.candidate_id,
            effective_job_id,
            Some(template.id),
            overall_score,
            overall_score_5,
            t0_assessment.score_5,
            t1_assessment.score_5,
            t2_assessment.score_5,
            t3_assessment.score_5,
            recommendation,
            normalized_risk_level,
            structured_result.to_string(),
            created_at,
        ],
    )
    .map_err(|error| error.to_string())?;

    let id = conn.last_insert_rowid();
    let result = ScoringResultRecord {
        id,
        candidate_id: input.candidate_id,
        job_id: effective_job_id,
        template_id: Some(template.id),
        overall_score,
        overall_score_5,
        t0_score_5: t0_assessment.score_5,
        t1_score_5: t1_assessment.score_5,
        t2_score_5: t2_assessment.score_5,
        t3_score_5: t3_assessment.score_5,
        recommendation,
        risk_level: normalized_risk_level,
        structured_result,
        created_at,
    };

    write_audit(
        &conn,
        "scoring.run",
        "scoring_result",
        Some(result.id.to_string()),
        serde_json::json!({
            "candidateId": result.candidate_id,
            "jobId": result.job_id,
            "templateId": result.template_id,
            "overallScore": result.overall_score,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(result)
}

pub(crate) fn run_candidate_ai_analysis_silent(
    state: &AppState,
    input: RunCandidateScoringInput,
) -> Result<ScoringResultRecord, String> {
    run_candidate_ai_analysis_blocking(state, input, |_| {})
}

#[tauri::command]
pub(crate) fn get_scoring_template(
    state: State<'_, AppState>,
    job_id: Option<i64>,
) -> Result<ScoringTemplateRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    resolve_scoring_template(&conn, job_id)
}

#[tauri::command]
pub(crate) fn upsert_scoring_template(
    state: State<'_, AppState>,
    input: UpsertScoringTemplateInput,
) -> Result<ScoringTemplateRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let scope = if input.job_id.is_some() {
        "job"
    } else {
        "global"
    };
    let config = normalize_scoring_template_config(input.config)?;
    let name = input
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if input.job_id.is_some() {
                "岗位评分模板".to_string()
            } else {
                "默认评分模板".to_string()
            }
        });

    let template = upsert_scoring_template_internal(&conn, scope, input.job_id, name, config)?;

    write_audit(
        &conn,
        "scoring.template.upsert",
        "scoring_template",
        Some(template.id.to_string()),
        serde_json::json!({
            "scope": template.scope,
            "jobId": template.job_id,
            "name": template.name,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(template)
}

#[tauri::command]
pub(crate) fn list_scoring_templates(
    state: State<'_, AppState>,
) -> Result<Vec<ScoringTemplateRecord>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut templates = list_global_scoring_templates(&conn)?;
    if templates.is_empty() {
        let default_template = upsert_scoring_template_internal(
            &conn,
            "global",
            None,
            "默认评分模板".to_string(),
            normalize_scoring_template_config(None)?,
        )?;
        templates = vec![default_template];
    }
    Ok(templates)
}

#[tauri::command]
pub(crate) fn create_scoring_template(
    state: State<'_, AppState>,
    input: CreateScoringTemplateInput,
) -> Result<ScoringTemplateRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let config = normalize_scoring_template_config(input.config)?;
    let name = input
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "新评分模板".to_string());

    let template = create_global_scoring_template_internal(&conn, name, config)?;

    write_audit(
        &conn,
        "scoring.template.create",
        "scoring_template",
        Some(template.id.to_string()),
        serde_json::json!({
            "scope": template.scope,
            "name": template.name,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(template)
}

#[tauri::command]
pub(crate) fn update_scoring_template(
    state: State<'_, AppState>,
    input: UpdateScoringTemplateInput,
) -> Result<ScoringTemplateRecord, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let existing = read_scoring_template_by_id(&conn, input.template_id)?;
    if existing.scope != "global" {
        return Err("scoring_template_scope_invalid".to_string());
    }

    let config = normalize_scoring_template_config(input.config.or(Some(existing.config.clone())))?;
    let name = input
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or(existing.name);

    let template = update_global_scoring_template_internal(&conn, input.template_id, name, config)?;

    write_audit(
        &conn,
        "scoring.template.update",
        "scoring_template",
        Some(template.id.to_string()),
        serde_json::json!({
            "scope": template.scope,
            "name": template.name,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(template)
}

#[tauri::command]
pub(crate) fn delete_scoring_template(
    state: State<'_, AppState>,
    template_id: i64,
) -> Result<Vec<ScoringTemplateRecord>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let existing = read_scoring_template_by_id(&conn, template_id)?;
    if existing.scope != "global" {
        return Err("scoring_template_scope_invalid".to_string());
    }

    let default_template_id = resolve_resident_default_global_template_id(&conn)?;
    if default_template_id == Some(template_id) {
        return Err("默认评分模板不可删除，请改为编辑模板内容".to_string());
    }

    let job_usage_count = count_jobs_using_scoring_template(&conn, template_id)?;
    if job_usage_count > 0 {
        return Err(format!(
            "该评分模板已被 {job_usage_count} 个职位使用，请先切换模板后再删除"
        ));
    }

    conn.execute("DELETE FROM scoring_templates WHERE id = ?1", [template_id])
        .map_err(|error| error.to_string())?;

    let templates = list_global_scoring_templates(&conn)?;

    write_audit(
        &conn,
        "scoring.template.delete",
        "scoring_template",
        Some(template_id.to_string()),
        serde_json::json!({
            "scope": existing.scope,
            "name": existing.name,
        }),
    )
    .map_err(|error| error.to_string())?;

    Ok(templates)
}

#[tauri::command]
pub(crate) fn set_job_scoring_template(
    state: State<'_, AppState>,
    input: SetJobScoringTemplateInput,
) -> Result<crate::models::job::Job, String> {
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
                "SELECT scope FROM scoring_templates WHERE id = ?1",
                [template_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("scoring_template_not_found:{template_id}"))?;

        if scope != "global" {
            return Err("scoring_template_scope_invalid".to_string());
        }

        conn.execute(
            r#"
            INSERT INTO job_scoring_overrides(job_id, template_id, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(job_id)
            DO UPDATE SET template_id = excluded.template_id, updated_at = excluded.updated_at
            "#,
            params![input.job_id, template_id, now, now],
        )
        .map_err(|error| error.to_string())?;
    } else {
        conn.execute(
            "DELETE FROM job_scoring_overrides WHERE job_id = ?1",
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
        "job.scoring_template.set",
        "job",
        Some(job.id.to_string()),
        serde_json::json!({ "templateId": input.template_id }),
    )
    .map_err(|error| error.to_string())?;

    Ok(job)
}

#[tauri::command]
pub(crate) async fn run_candidate_ai_analysis(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    input: RunCandidateScoringInput,
) -> Result<ScoringResultRecord, String> {
    let run_id = input
        .run_id
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("ai-analysis-{}-{}", input.candidate_id, now_iso()));

    let candidate_id = input.candidate_id;
    let app_state = state.inner().clone();
    let app_handle_for_task = app_handle.clone();
    let run_id_for_task = run_id.clone();
    let input_for_task = input.clone();

    let task_result = tauri::async_runtime::spawn_blocking(move || {
        let mut last_phase = "prepare".to_string();
        let result = run_candidate_ai_analysis_blocking(&app_state, input_for_task, |update| {
            last_phase = update.phase.to_string();
            emit_scoring_progress(&app_handle_for_task, &run_id_for_task, candidate_id, update);
        });
        (result, last_phase)
    })
    .await
    .map_err(|error| {
        let message = format!("scoring_task_join_error: {error}");
        emit_scoring_progress(
            &app_handle,
            &run_id,
            candidate_id,
            scoring_progress_update("persist", "failed", "end", message.clone(), None),
        );
        message
    })?;

    let (result, last_phase) = task_result;
    match result {
        Ok(record) => {
            emit_scoring_progress(
                &app_handle,
                &run_id,
                candidate_id,
                scoring_progress_update(
                    "persist",
                    "completed",
                    "end",
                    "评分完成并已刷新结果",
                    None,
                ),
            );
            Ok(record)
        }
        Err(error) => {
            let phase = match last_phase.as_str() {
                "t0" => "t0",
                "t1" => "t1",
                "t2" => "t2",
                "t3" => "t3",
                "persist" => "persist",
                _ => "prepare",
            };
            emit_scoring_progress(
                &app_handle,
                &run_id,
                candidate_id,
                scoring_progress_update(phase, "failed", "end", error.clone(), None),
            );
            Err(error)
        }
    }
}

#[tauri::command]
pub(crate) fn list_scoring_results(
    state: State<'_, AppState>,
    candidate_id: i64,
) -> Result<Vec<ScoringResultRecord>, String> {
    let conn = open_connection(&state.db_path).map_err(|error| error.to_string())?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, candidate_id, job_id, template_id,
                   overall_score, overall_score_5,
                   t0_score_5, t1_score_5, t2_score_5, t3_score_5,
                   recommendation, risk_level,
                   structured_result_json, created_at
            FROM scoring_results
            WHERE candidate_id = ?1
            ORDER BY created_at DESC, id DESC
            "#,
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([candidate_id], |row| {
            let structured_text: String = row.get(12)?;
            let structured_result = serde_json::from_str(&structured_text).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    12,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;

            Ok(ScoringResultRecord {
                id: row.get(0)?,
                candidate_id: row.get(1)?,
                job_id: row.get(2)?,
                template_id: row.get(3)?,
                overall_score: row.get(4)?,
                overall_score_5: row.get(5)?,
                t0_score_5: row.get(6)?,
                t1_score_5: row.get(7)?,
                t2_score_5: row.get(8)?,
                t3_score_5: row.get(9)?,
                recommendation: row.get(10)?,
                risk_level: row.get(11)?,
                structured_result,
                created_at: row.get(13)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_context() -> CandidateScoringContext {
        CandidateScoringContext {
            candidate_years: 4.0,
            candidate_stage: "SCREENING".to_string(),
            candidate_tags: vec!["vue".to_string()],
            resume_raw_text: "候选人具备完整项目经验与技能信息".to_string(),
            resume_parsed: serde_json::json!({}),
            resume_lower: "候选人具备完整项目经验与技能信息".to_string(),
            required_skills: vec!["vue".to_string()],
            extracted_skills: vec!["Vue".to_string()],
            normalized_skills: vec!["vue".to_string()],
            matched_skill_count: 1,
            skill_coverage: 1.0,
            expected_salary_k: Some(35.0),
            max_salary_k: Some(40.0),
            project_mentions: 2,
        }
    }

    fn build_test_template() -> ScoringTemplateRecord {
        ScoringTemplateRecord {
            id: 1,
            scope: "global".to_string(),
            job_id: None,
            name: "测试模板".to_string(),
            config: default_scoring_template_config(),
            created_at: "2026-03-01T00:00:00Z".to_string(),
            updated_at: "2026-03-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn normalize_comment_should_not_truncate_text() {
        let raw = "这是一个用于验证不会被截断的超长文本";
        let value = normalize_comment(raw, "fallback");
        assert_eq!(value, raw);
    }

    #[test]
    fn build_section_assessment_reason_should_not_use_generic_fallback() {
        let section = ScoringSectionConfig {
            items: vec![ScoringItemConfig {
                key: "goal_orientation".to_string(),
                label: "目标导向".to_string(),
                description: "".to_string(),
                weight: 100,
            }],
        };

        let result = build_section_assessment("t1", &section, None, &build_test_context());
        assert_eq!(result.items.len(), 1);
        assert_ne!(result.items[0].reason, "基于候选人资料与岗位要求自动评估。");
    }

    #[test]
    fn build_scoring_prompts_should_require_evidence_based_outputs() {
        let (system_prompt, _user_prompt) =
            build_scoring_prompts(&build_test_template(), &build_test_context());
        assert!(system_prompt.contains("简历证据点+判断结论"));
        assert!(system_prompt.contains("模块小结"));
        assert!(system_prompt.contains("整体评价"));
    }

    #[test]
    fn t0_skill_reason_should_reference_match_ratio() {
        let section = ScoringSectionConfig {
            items: vec![ScoringItemConfig {
                key: "required_skills_match".to_string(),
                label: "岗位技能匹配".to_string(),
                description: "".to_string(),
                weight: 100,
            }],
        };

        let result = build_section_assessment("t0", &section, None, &build_test_context());
        assert_eq!(result.items.len(), 1);
        assert!(result.items[0].reason.contains("/"));
    }

    #[test]
    fn section_comment_fallback_should_include_module_summary_and_advice() {
        let section = ScoringSectionConfig {
            items: vec![
                ScoringItemConfig {
                    key: "goal_orientation".to_string(),
                    label: "目标导向".to_string(),
                    description: "".to_string(),
                    weight: 50,
                },
                ScoringItemConfig {
                    key: "team_collaboration".to_string(),
                    label: "团队协作".to_string(),
                    description: "".to_string(),
                    weight: 50,
                },
            ],
        };

        let result = build_section_assessment("t1", &section, None, &build_test_context());
        assert!(result.comment.contains("小结"));
        assert!(result.comment.contains("优势"));
        assert!(result.comment.contains("建议"));
    }

    #[test]
    fn overall_comment_fallback_should_include_resume_context_and_suggestion() {
        let ctx = build_test_context();
        let template = build_test_template();
        let t0 = SectionAssessment {
            score_5: 4.2,
            items: vec![],
            comment: "T0小结".to_string(),
        };
        let t1 = SectionAssessment {
            score_5: 4.0,
            items: vec![],
            comment: "T1小结".to_string(),
        };
        let t2 = SectionAssessment {
            score_5: 3.9,
            items: vec![],
            comment: "T2小结".to_string(),
        };
        let t3 = SectionAssessment {
            score_5: 3.3,
            items: vec![],
            comment: "T3小结".to_string(),
        };
        let comment = build_overall_comment_fallback(
            &ctx,
            &template.config.weights,
            4.0,
            80,
            &t0,
            &t1,
            &t2,
            &t3,
            "REVIEW",
            "MEDIUM",
        );

        assert!(comment.contains("年经验"));
        assert!(comment.contains("技能匹配"));
        assert!(comment.contains("建议"));
    }
}
