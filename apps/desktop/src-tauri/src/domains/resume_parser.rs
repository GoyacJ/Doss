use chrono::Utc;
use regex::Regex;
use serde_json::Value;
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use zip::ZipArchive;

use crate::core::time::now_iso;
use crate::models::resume::{
    ResumeBasicInfo, ResumeDerivedMetrics, ResumeEducationItem, ResumeLanguageItem, ResumeParseMeta,
    ResumeParsedV2, ResumeProjectItem, ResumeSection, ResumeWorkExperienceItem,
    RESUME_PARSER_VERSION, RESUME_SCHEMA_VERSION_V2,
};

pub(crate) fn normalize_resume_text(text: &str) -> String {
    text.replace('\u{00a0}', " ")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn decode_xml_entities(text: &str) -> String {
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

pub(crate) fn extract_resume_text_from_bytes(
    file_name: &str,
    bytes: &[u8],
    enable_ocr: bool,
) -> Result<(String, bool, String), String> {
    let extension = extract_file_extension(file_name);
    let mut raw_text = match extension.as_str() {
        "pdf" => extract_text_from_pdf_bytes(bytes),
        "docx" => extract_text_from_docx_bytes(bytes),
        "txt" | "md" => extract_text_from_plain_bytes(bytes),
        "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff" => Ok(String::new()),
        _ => Err(format!("unsupported_resume_file_type: {}", extension)),
    }?;

    let mut ocr_used = false;
    if enable_ocr
        && raw_text.trim().is_empty()
        && matches!(
            extension.as_str(),
            "pdf" | "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff"
        )
    {
        if let Ok(ocr_text) = try_tesseract_ocr(bytes, &extension) {
            if !ocr_text.trim().is_empty() {
                raw_text = ocr_text;
                ocr_used = true;
            }
        }
    }

    Ok((normalize_resume_text(&raw_text), ocr_used, extension))
}

fn build_section_title(line: &str) -> String {
    line.trim().trim_matches(':').trim_matches('：').to_string()
}

fn is_section_heading(line: &str) -> bool {
    let normalized = line.trim();
    if normalized.len() < 2 || normalized.len() > 24 {
        return false;
    }
    let lowered = normalized.to_lowercase();
    let keywords = [
        "工作经历",
        "工作经验",
        "项目经历",
        "项目经验",
        "教育经历",
        "教育背景",
        "技能",
        "专业技能",
        "证书",
        "语言",
        "自我评价",
        "个人优势",
        "experience",
        "education",
        "skills",
        "projects",
        "certification",
        "language",
        "summary",
    ];
    keywords.iter().any(|item| lowered.contains(item))
}

fn split_resume_sections(raw_text: &str) -> Vec<ResumeSection> {
    let lines = raw_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return Vec::new();
    }

    let mut sections = Vec::<ResumeSection>::new();
    let mut current_title = "概览".to_string();
    let mut current_key = "overview".to_string();
    let mut current_content = Vec::<String>::new();

    for line in lines {
        let title_line = line.trim_matches(|ch| matches!(ch, '-' | '•' | '·' | '*')).trim();
        if is_section_heading(title_line) {
            if !current_content.is_empty() {
                sections.push(ResumeSection {
                    key: current_key.clone(),
                    title: current_title.clone(),
                    content: current_content.join("\n"),
                });
                current_content.clear();
            }
            let title = build_section_title(title_line);
            current_key = title
                .chars()
                .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                .collect::<String>()
                .to_lowercase();
            if current_key.is_empty() {
                current_key = format!("section_{}", sections.len() + 1);
            }
            current_title = title;
            continue;
        }
        current_content.push(line.to_string());
    }

    if !current_content.is_empty() {
        sections.push(ResumeSection {
            key: current_key,
            title: current_title,
            content: current_content.join("\n"),
        });
    }

    if sections.is_empty() {
        sections.push(ResumeSection {
            key: "overview".to_string(),
            title: "概览".to_string(),
            content: raw_text.to_string(),
        });
    }

    sections
}

fn extract_skills(raw_text: &str) -> Vec<String> {
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
        ("Docker", &["docker"]),
        ("Kubernetes", &["k8s", "kubernetes"]),
        ("Redis", &["redis"]),
        ("LLM", &["llm", "大模型", "prompt"]),
    ];

    let mut skills = Vec::<String>::new();
    for (label, keywords) in skill_catalog {
        if keywords.iter().any(|keyword| lowered.contains(keyword)) {
            skills.push(label.to_string());
        }
    }
    skills
}

fn extract_years_of_experience(raw_text: &str) -> Option<f64> {
    let years_regex = Regex::new(r"(?i)(\d{1,2}(?:\.\d+)?)\s*年").expect("years regex");
    let years = years_regex
        .captures_iter(raw_text)
        .filter_map(|capture| {
            capture
                .get(1)
                .and_then(|value| value.as_str().parse::<f64>().ok())
        })
        .fold(0.0_f64, f64::max);
    if years > 0.0 {
        Some(years)
    } else {
        None
    }
}

fn extract_expected_salary_k(raw_text: &str) -> Option<f64> {
    let salary_context_regex = Regex::new(
        r"(?i)(?:期望薪资|期望|薪资|薪酬|salary)[^\d]{0,8}(\d{1,3})(?:\s*[-~到]\s*(\d{1,3}))?\s*[kK千]",
    )
    .expect("salary context regex");
    let generic_salary_regex = Regex::new(r"(?i)\b(\d{1,3})\s*[kK千]\b").expect("salary regex");
    salary_context_regex
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
        })
}

fn extract_email(raw_text: &str) -> Option<String> {
    let regex = Regex::new(r"(?i)([a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,})").ok()?;
    regex
        .captures(raw_text)
        .and_then(|capture| capture.get(1))
        .map(|item| item.as_str().to_string())
}

fn extract_phone(raw_text: &str) -> Option<String> {
    let regex = Regex::new(r"(?x)(\+?\d[\d\-\s]{6,}\d)").ok()?;
    regex
        .captures(raw_text)
        .and_then(|capture| capture.get(1))
        .map(|item| item.as_str().trim().to_string())
}

fn extract_name(raw_text: &str) -> Option<String> {
    let first_line = raw_text.lines().next().unwrap_or("").trim();
    if first_line.chars().count() <= 12 && !first_line.contains('@') {
        let has_cn = first_line.chars().any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch));
        if has_cn {
            return Some(first_line.to_string());
        }
    }
    None
}

fn extract_summary(raw_text: &str) -> Option<String> {
    let summary = raw_text.chars().take(360).collect::<String>();
    if summary.trim().is_empty() {
        None
    } else {
        Some(summary)
    }
}

fn detect_education_level(raw_text: &str) -> Option<String> {
    if raw_text.contains("博士") {
        Some("博士".to_string())
    } else if raw_text.contains("硕士") {
        Some("硕士".to_string())
    } else if raw_text.contains("本科") {
        Some("本科".to_string())
    } else if raw_text.contains("大专") {
        Some("大专".to_string())
    } else {
        None
    }
}

fn extract_schools(raw_text: &str) -> Vec<String> {
    let school_regex = Regex::new(r"([^\s]{2,16}(大学|学院))").expect("school regex");
    school_regex
        .captures_iter(raw_text)
        .filter_map(|capture| capture.get(1).map(|value| value.as_str().to_string()))
        .take(5)
        .collect::<Vec<_>>()
}

fn extract_education_items(raw_text: &str) -> Vec<ResumeEducationItem> {
    let level = detect_education_level(raw_text);
    extract_schools(raw_text)
        .into_iter()
        .map(|school| ResumeEducationItem {
            school: Some(school),
            degree: level.clone(),
            major: None,
            start: None,
            end: None,
            description: None,
        })
        .collect()
}

fn extract_work_items(sections: &[ResumeSection]) -> Vec<ResumeWorkExperienceItem> {
    sections
        .iter()
        .filter(|section| section.title.contains("工作") || section.key.contains("experience"))
        .map(|section| ResumeWorkExperienceItem {
            company: None,
            title: Some(section.title.clone()),
            start: None,
            end: None,
            summary: Some(section.content.chars().take(300).collect()),
        })
        .collect()
}

fn extract_project_items(sections: &[ResumeSection]) -> Vec<ResumeProjectItem> {
    sections
        .iter()
        .filter(|section| section.title.contains("项目") || section.key.contains("project"))
        .map(|section| ResumeProjectItem {
            name: Some(section.title.clone()),
            role: None,
            start: None,
            end: None,
            summary: Some(section.content.chars().take(320).collect()),
        })
        .collect()
}

fn extract_languages(raw_text: &str) -> Vec<ResumeLanguageItem> {
    let candidates = [("中文", Some("母语")), ("英语", None), ("日语", None)];
    candidates
        .iter()
        .filter(|(name, _)| raw_text.contains(*name))
        .map(|(name, level)| ResumeLanguageItem {
            name: (*name).to_string(),
            level: level.map(|value| value.to_string()),
        })
        .collect()
}

fn ensure_non_empty_sections(raw_text: &str, sections: Vec<ResumeSection>) -> Vec<ResumeSection> {
    if !sections.is_empty() {
        return sections;
    }
    if raw_text.trim().is_empty() {
        return vec![ResumeSection {
            key: "overview".to_string(),
            title: "概览".to_string(),
            content: String::new(),
        }];
    }
    vec![ResumeSection {
        key: "overview".to_string(),
        title: "概览".to_string(),
        content: raw_text.to_string(),
    }]
}

pub(crate) fn parse_resume_text_v2(
    raw_text: &str,
    source: &str,
    ocr_used: bool,
    parsed_at: Option<String>,
) -> ResumeParsedV2 {
    let normalized = normalize_resume_text(raw_text);
    let sections = ensure_non_empty_sections(&normalized, split_resume_sections(&normalized));
    let skills = extract_skills(&normalized);
    let education = extract_education_items(&normalized);
    let work_experiences = extract_work_items(&sections);
    let projects = extract_project_items(&sections);
    let languages = extract_languages(&normalized);
    let basic = ResumeBasicInfo {
        name: extract_name(&normalized),
        email: extract_email(&normalized),
        phone: extract_phone(&normalized),
        location: None,
        title: None,
        summary: extract_summary(&normalized),
        years_of_experience: extract_years_of_experience(&normalized),
        expected_salary_k: extract_expected_salary_k(&normalized),
    };

    ResumeParsedV2 {
        schema_version: RESUME_SCHEMA_VERSION_V2,
        parse_meta: ResumeParseMeta {
            parser_version: RESUME_PARSER_VERSION.to_string(),
            parsed_at: parsed_at.unwrap_or_else(now_iso),
            source: source.to_string(),
            ocr_used,
            text_length: normalized.chars().count(),
            section_count: sections.len(),
        },
        basic,
        skills: skills.clone(),
        education: education.clone(),
        work_experiences: work_experiences.clone(),
        projects: projects.clone(),
        certificates: Vec::new(),
        languages,
        sections: sections.clone(),
        derived_metrics: ResumeDerivedMetrics {
            project_count: projects.len(),
            work_experience_count: work_experiences.len(),
            education_count: education.len(),
            skill_count: skills.len(),
            section_count: sections.len(),
            text_chars: normalized.chars().count(),
        },
    }
}

fn get_number(value: Option<&Value>) -> Option<f64> {
    value.and_then(|item| {
        item.as_f64().or_else(|| {
            item.as_i64().map(|number| number as f64).or_else(|| {
                item.as_str()
                    .and_then(|text| text.trim().parse::<f64>().ok())
            })
        })
    })
}

pub(crate) fn parse_skills_from_parsed_json(parsed: &Value) -> Vec<String> {
    parsed
        .get("skills")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::trim))
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub(crate) fn expected_salary_k_from_parsed_json(parsed: &Value) -> Option<f64> {
    if let Some(value) = get_number(parsed.get("expectedSalaryK")) {
        return Some(value);
    }
    get_number(
        parsed
            .get("basic")
            .and_then(|value| value.get("expected_salary_k")),
    )
}

pub(crate) fn project_mentions_from_parsed_json(parsed: &Value) -> i64 {
    if let Some(value) = parsed.get("projectMentions").and_then(|item| item.as_i64()) {
        return value.max(0);
    }
    parsed
        .get("projects")
        .and_then(|value| value.as_array())
        .map(|items| items.len() as i64)
        .unwrap_or(0)
}
