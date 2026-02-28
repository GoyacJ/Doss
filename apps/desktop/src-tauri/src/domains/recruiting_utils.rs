use serde_json::Value;

use crate::domains::ai_runtime::trim_resume_excerpt;
use crate::models::interview::{InterviewEvaluationPayload, InterviewQuestion};

pub(crate) fn clamp_score(value: i32) -> i32 {
    value.clamp(0, 100)
}

pub(crate) fn round_one_decimal(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
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

fn collect_numeric_scores(value: &Value, scores: &mut Vec<f64>) {
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

fn collect_string_values(value: &Value) -> Vec<String> {
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

fn build_interview_evidence(transcript: &str) -> Vec<String> {
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
    scoring_recommendation: Option<&str>,
    scoring_risk_level: Option<&str>,
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
            red_flags: vec!["只讲理念，不给执行路径".to_string(), "忽略风险与兜底机制".to_string()],
        });
    }

    let risk_topic = if scoring_risk_level == Some("HIGH")
        || scoring_recommendation == Some("REVIEW")
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
        .map(|value| if value > 0.0 && value <= 1.0 { value * 5.0 } else { value })
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
