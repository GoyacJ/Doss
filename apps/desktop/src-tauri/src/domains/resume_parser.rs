use calamine::{open_workbook_auto, Reader};
use chrono::Utc;
use csv::ReaderBuilder;
use quick_xml::events::Event;
use quick_xml::Reader as XmlReader;
use regex::Regex;
use serde_json::Value;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zip::ZipArchive;

use crate::core::time::now_iso;
use crate::models::candidate::{
    ResumeProfileFieldInt, ResumeProfileFieldNumber, ResumeProfileFieldText,
    ResumeProfilePreviewExtracted,
};
use crate::models::resume::{
    ResumeBasicInfo, ResumeDerivedMetrics, ResumeEducationItem, ResumeLanguageItem,
    ResumeParseMeta, ResumeParsedV2, ResumeProjectItem, ResumeSection, ResumeWorkExperienceItem,
    RESUME_PARSER_VERSION, RESUME_SCHEMA_VERSION_V2,
};

#[derive(Debug, Clone)]
pub(crate) struct ResumeTextExtraction {
    pub(crate) canonical_markdown: String,
    pub(crate) plain_text: String,
    pub(crate) extension: String,
    pub(crate) ocr_used: bool,
    pub(crate) warnings: Vec<String>,
    pub(crate) content_format: String,
}

pub(crate) fn resume_parser_v3_enabled() -> bool {
    std::env::var("RESUME_PARSER_V3")
        .ok()
        .map(|value| {
            let normalized = value.trim().to_lowercase();
            !(normalized == "0" || normalized == "false" || normalized == "off")
        })
        .unwrap_or(true)
}

pub(crate) fn normalize_resume_text(text: &str) -> String {
    let normalized_newlines = text
        .replace('\u{00a0}', " ")
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut result = String::new();
    let mut empty_streak = 0;

    for line in normalized_newlines.lines() {
        let trimmed_end = line.trim_end();
        if trimmed_end.is_empty() {
            empty_streak += 1;
            if empty_streak <= 2 {
                if !result.is_empty() {
                    result.push('\n');
                }
            }
            continue;
        }

        empty_streak = 0;
        if !result.is_empty() && !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(trimmed_end);
    }

    result.trim().to_string()
}

fn decode_xml_entities(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn xml_local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}

fn append_docx_text(paragraph: &mut String, cell: &mut String, in_cell: bool, text: &str) {
    if text.is_empty() {
        return;
    }

    if in_cell {
        cell.push_str(text);
    } else {
        paragraph.push_str(text);
    }
}

fn markdown_escape_cell(text: &str) -> String {
    text.replace('|', "\\|")
}

fn markdown_table_from_rows(rows: &[Vec<String>]) -> String {
    if rows.is_empty() {
        return String::new();
    }

    let width = rows.iter().map(|row| row.len()).max().unwrap_or(0).max(1);
    let mut normalized_rows = rows
        .iter()
        .map(|row| {
            let mut cells = row.clone();
            while cells.len() < width {
                cells.push(String::new());
            }
            cells
        })
        .collect::<Vec<_>>();

    if normalized_rows.is_empty() {
        return String::new();
    }

    let mut header = normalized_rows.remove(0);
    if header.iter().all(|item| item.trim().is_empty()) {
        header = (1..=width).map(|index| format!("Column{index}")).collect();
    }

    let mut lines = Vec::<String>::new();
    lines.push(format!(
        "| {} |",
        header
            .iter()
            .map(|item| markdown_escape_cell(item.trim()))
            .collect::<Vec<_>>()
            .join(" | ")
    ));
    lines.push(format!("| {} |", vec!["---"; width].join(" | ")));

    for row in normalized_rows {
        lines.push(format!(
            "| {} |",
            row.iter()
                .map(|item| markdown_escape_cell(item.trim()))
                .collect::<Vec<_>>()
                .join(" | ")
        ));
    }

    lines.join("\n")
}

fn legacy_extract_docx_xml_text(xml_bytes: &[u8]) -> Result<String, String> {
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

fn legacy_extract_text_from_docx_bytes(bytes: &[u8]) -> Result<String, String> {
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
            let text = legacy_extract_docx_xml_text(&xml)?;
            if !text.trim().is_empty() {
                sections.push(text);
            }
        }
    }
    Ok(normalize_resume_text(&sections.join("\n")))
}

fn legacy_extract_resume_text_from_bytes(
    file_name: &str,
    bytes: &[u8],
    enable_ocr: bool,
) -> Result<(String, bool, String), String> {
    let extension = extract_file_extension(file_name);
    let mut raw_text = match extension.as_str() {
        "pdf" => extract_text_from_pdf_bytes(bytes),
        "docx" => legacy_extract_text_from_docx_bytes(bytes),
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

pub(crate) fn extract_docx_xml_text(xml_bytes: &[u8]) -> Result<String, String> {
    let mut reader = XmlReader::from_reader(Cursor::new(xml_bytes));
    reader.config_mut().trim_text(false);

    let mut blocks = Vec::<String>::new();
    let mut paragraph = String::new();
    let mut current_cell = String::new();
    let mut current_row = Vec::<String>::new();
    let mut table_rows = Vec::<Vec<String>>::new();
    let mut in_table = false;
    let mut in_cell = false;
    let mut buffer = Vec::<u8>::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => match xml_local_name(event.name().as_ref()) {
                b"tbl" => {
                    if !paragraph.trim().is_empty() {
                        blocks.push(paragraph.trim().to_string());
                        paragraph.clear();
                    }
                    in_table = true;
                    table_rows.clear();
                }
                b"tr" => {
                    if in_table {
                        current_row.clear();
                    }
                }
                b"tc" => {
                    if in_table {
                        in_cell = true;
                        current_cell.clear();
                    }
                }
                b"br" => append_docx_text(&mut paragraph, &mut current_cell, in_cell, "\n"),
                b"tab" => append_docx_text(&mut paragraph, &mut current_cell, in_cell, "\t"),
                _ => {}
            },
            Ok(Event::Empty(event)) => match xml_local_name(event.name().as_ref()) {
                b"br" => append_docx_text(&mut paragraph, &mut current_cell, in_cell, "\n"),
                b"tab" => append_docx_text(&mut paragraph, &mut current_cell, in_cell, "\t"),
                _ => {}
            },
            Ok(Event::Text(event)) => {
                let text = event.decode().map_err(|error| error.to_string())?;
                let decoded = decode_xml_entities(text.as_ref());
                append_docx_text(&mut paragraph, &mut current_cell, in_cell, &decoded);
            }
            Ok(Event::CData(event)) => {
                let text = event.decode().map_err(|error| error.to_string())?;
                append_docx_text(&mut paragraph, &mut current_cell, in_cell, text.as_ref());
            }
            Ok(Event::End(event)) => match xml_local_name(event.name().as_ref()) {
                b"p" => {
                    if in_table {
                        if in_cell && !current_cell.ends_with('\n') {
                            current_cell.push('\n');
                        }
                    } else {
                        let value = paragraph.trim();
                        if !value.is_empty() {
                            blocks.push(value.to_string());
                        }
                        paragraph.clear();
                    }
                }
                b"tc" => {
                    if in_table {
                        in_cell = false;
                        let text = current_cell
                            .lines()
                            .map(str::trim)
                            .filter(|item| !item.is_empty())
                            .collect::<Vec<_>>()
                            .join(" ");
                        current_row.push(text);
                        current_cell.clear();
                    }
                }
                b"tr" => {
                    if in_table && !current_row.is_empty() {
                        table_rows.push(current_row.clone());
                        current_row.clear();
                    }
                }
                b"tbl" => {
                    if in_table {
                        let table_markdown = markdown_table_from_rows(&table_rows);
                        if !table_markdown.trim().is_empty() {
                            blocks.push(table_markdown);
                        }
                        table_rows.clear();
                        in_table = false;
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(error) => return Err(error.to_string()),
            _ => {}
        }
        buffer.clear();
    }

    if !paragraph.trim().is_empty() {
        blocks.push(paragraph.trim().to_string());
    }

    Ok(normalize_resume_text(&blocks.join("\n\n")))
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

    Ok(normalize_resume_text(&sections.join("\n\n")))
}

pub(crate) fn extract_text_from_pdf_bytes(bytes: &[u8]) -> Result<String, String> {
    let text = pdf_extract::extract_text_from_mem(bytes).map_err(|error| error.to_string())?;
    Ok(normalize_resume_text(&text))
}

pub(crate) fn extract_text_from_plain_bytes(bytes: &[u8]) -> Result<String, String> {
    let text = String::from_utf8(bytes.to_vec())
        .unwrap_or_else(|_| String::from_utf8_lossy(bytes).to_string());
    Ok(normalize_resume_text(&text))
}

fn extract_text_from_csv_bytes(bytes: &[u8]) -> Result<String, String> {
    let text = String::from_utf8(bytes.to_vec())
        .unwrap_or_else(|_| String::from_utf8_lossy(bytes).to_string());
    let mut reader = ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());

    let mut rows = Vec::<Vec<String>>::new();
    for record in reader.records() {
        let item = record.map_err(|error| error.to_string())?;
        rows.push(item.iter().map(str::to_string).collect());
    }

    Ok(normalize_resume_text(&markdown_table_from_rows(&rows)))
}

fn spreadsheet_cell_to_string<T: ToString>(value: &T) -> String {
    value.to_string().trim_end_matches(".0").to_string()
}

fn write_temp_file(bytes: &[u8], extension: &str) -> Result<PathBuf, String> {
    let token = format!(
        "doss-resume-{}-{}",
        Utc::now().timestamp_millis(),
        rand::random::<u32>()
    );
    let path = std::env::temp_dir().join(format!("{token}.{extension}"));
    fs::write(&path, bytes).map_err(|error| error.to_string())?;
    Ok(path)
}

fn extract_text_from_spreadsheet_bytes(bytes: &[u8], extension: &str) -> Result<String, String> {
    let temp_path = write_temp_file(bytes, extension)?;
    let result = (|| {
        let mut workbook = open_workbook_auto(&temp_path).map_err(|error| error.to_string())?;
        let names = workbook.sheet_names().to_vec();
        let mut sections = Vec::<String>::new();

        for name in names {
            let Ok(range) = workbook.worksheet_range(&name) else {
                continue;
            };
            if range.is_empty() {
                continue;
            }

            let width = range.width().max(1);
            let mut rows = Vec::<Vec<String>>::new();
            for row in range.rows() {
                let mut values = Vec::<String>::new();
                for index in 0..width {
                    let text = row
                        .get(index)
                        .map(spreadsheet_cell_to_string)
                        .unwrap_or_default();
                    values.push(text);
                }
                rows.push(values);
            }

            let table = markdown_table_from_rows(&rows);
            if !table.trim().is_empty() {
                sections.push(format!("## Sheet: {}\n{}", name, table));
            }
        }

        Ok(normalize_resume_text(&sections.join("\n\n")))
    })();

    let _ = fs::remove_file(&temp_path);
    result
}

fn extract_text_from_doc_bytes(bytes: &[u8]) -> Result<(String, Vec<String>), String> {
    let input_path = write_temp_file(bytes, "doc")?;
    let mut warnings = Vec::<String>::new();

    let antiword_output = Command::new("antiword").arg(&input_path).output();

    if let Ok(output) = antiword_output {
        if output.status.success() {
            let text = String::from_utf8(output.stdout)
                .unwrap_or_else(|_| String::from_utf8_lossy(&output.stderr).to_string());
            let normalized = normalize_resume_text(&text);
            if !normalized.is_empty() {
                let _ = fs::remove_file(&input_path);
                return Ok((normalized, warnings));
            }
        } else {
            warnings.push("doc_antiword_failed".to_string());
        }
    } else {
        warnings.push("doc_antiword_unavailable".to_string());
    }

    let libreoffice_status = Command::new("libreoffice")
        .arg("--headless")
        .arg("--convert-to")
        .arg("docx")
        .arg("--outdir")
        .arg(std::env::temp_dir())
        .arg(&input_path)
        .status();

    match libreoffice_status {
        Ok(status) if status.success() => {
            let output_docx = input_path.with_extension("docx");
            if output_docx.exists() {
                let docx_bytes = fs::read(&output_docx).map_err(|error| error.to_string())?;
                let parsed = extract_text_from_docx_bytes(&docx_bytes)?;
                let _ = fs::remove_file(&output_docx);
                let _ = fs::remove_file(&input_path);
                if !parsed.trim().is_empty() {
                    return Ok((parsed, warnings));
                }
            }
            warnings.push("doc_libo_generated_empty".to_string());
        }
        Ok(_) => warnings.push("doc_libo_failed".to_string()),
        Err(_) => warnings.push("doc_libo_unavailable".to_string()),
    }

    let _ = fs::remove_file(&input_path);
    Err(format!("doc_conversion_failed: {}", warnings.join(",")))
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

pub(crate) fn extract_resume_content_from_bytes(
    file_name: &str,
    bytes: &[u8],
    enable_ocr: bool,
) -> Result<ResumeTextExtraction, String> {
    let extension = extract_file_extension(file_name);
    let mut warnings = Vec::<String>::new();

    let mut canonical_markdown = match extension.as_str() {
        "pdf" => extract_text_from_pdf_bytes(bytes),
        "docx" => extract_text_from_docx_bytes(bytes),
        "doc" => extract_text_from_doc_bytes(bytes).map(|(text, doc_warnings)| {
            warnings.extend(doc_warnings);
            text
        }),
        "xls" | "xlsx" => extract_text_from_spreadsheet_bytes(bytes, &extension),
        "csv" => extract_text_from_csv_bytes(bytes),
        "txt" | "md" => extract_text_from_plain_bytes(bytes),
        "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff" => Ok(String::new()),
        _ => Err(format!("unsupported_resume_file_type: {}", extension)),
    }?;

    let mut ocr_used = false;
    if enable_ocr
        && canonical_markdown.trim().is_empty()
        && matches!(
            extension.as_str(),
            "pdf" | "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff"
        )
    {
        match try_tesseract_ocr(bytes, &extension) {
            Ok(ocr_text) if !ocr_text.trim().is_empty() => {
                canonical_markdown = ocr_text;
                ocr_used = true;
            }
            Ok(_) => warnings.push("ocr_empty_text".to_string()),
            Err(error) => warnings.push(error),
        }
    }

    let content_format = match extension.as_str() {
        "xls" | "xlsx" | "csv" => "table",
        "md" | "docx" | "doc" => "markdown",
        _ => "plain",
    }
    .to_string();

    let normalized_markdown = normalize_resume_text(&canonical_markdown);
    let plain_text = normalize_resume_text(&normalized_markdown);

    Ok(ResumeTextExtraction {
        canonical_markdown: normalized_markdown,
        plain_text,
        extension,
        ocr_used,
        warnings,
        content_format,
    })
}

pub(crate) fn extract_resume_text_from_bytes(
    file_name: &str,
    bytes: &[u8],
    enable_ocr: bool,
) -> Result<(String, bool, String), String> {
    if !resume_parser_v3_enabled() {
        return legacy_extract_resume_text_from_bytes(file_name, bytes, enable_ocr);
    }

    let extracted = extract_resume_content_from_bytes(file_name, bytes, enable_ocr)?;
    Ok((
        normalize_resume_text(&extracted.canonical_markdown),
        extracted.ocr_used,
        extracted.extension,
    ))
}

fn build_section_title(line: &str) -> String {
    let trimmed = line.trim();
    let without_hash = trimmed.trim_start_matches('#').trim();
    without_hash
        .trim_matches(':')
        .trim_matches('：')
        .trim()
        .to_string()
}

fn is_section_heading(line: &str) -> bool {
    let normalized = line.trim();
    if normalized.is_empty() {
        return false;
    }

    let without_hash = normalized.trim_start_matches('#').trim();
    if without_hash.len() < 2 || without_hash.len() > 36 {
        return false;
    }

    let lowered = without_hash.to_lowercase();
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

    normalized.starts_with('#') || keywords.iter().any(|item| lowered.contains(item))
}

fn split_resume_sections(raw_text: &str) -> Vec<ResumeSection> {
    let lines = raw_text.lines().map(str::trim_end).collect::<Vec<_>>();
    if lines.is_empty() {
        return Vec::new();
    }

    let mut sections = Vec::<ResumeSection>::new();
    let mut current_title = "概览".to_string();
    let mut current_key = "overview".to_string();
    let mut current_content = Vec::<String>::new();

    for raw_line in lines {
        let line = raw_line.trim();
        if line.is_empty() {
            current_content.push(String::new());
            continue;
        }

        let title_line = line
            .trim_matches(|ch| matches!(ch, '-' | '•' | '·' | '*'))
            .trim();
        if is_section_heading(title_line) {
            if !current_content.is_empty() {
                sections.push(ResumeSection {
                    key: current_key.clone(),
                    title: current_title.clone(),
                    content: normalize_resume_text(&current_content.join("\n")),
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

        current_content.push(raw_line.to_string());
    }

    if !current_content.is_empty() {
        sections.push(ResumeSection {
            key: current_key,
            title: current_title,
            content: normalize_resume_text(&current_content.join("\n")),
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
    if first_line.chars().count() <= 20 && !first_line.contains('@') {
        let has_cn = first_line
            .chars()
            .any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch));
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
            content_format: "plain".to_string(),
            source_extension: None,
            warnings: Vec::new(),
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

fn confidence_level(confidence: f64) -> String {
    if confidence >= 0.85 {
        "HIGH".to_string()
    } else if confidence >= 0.6 {
        "MEDIUM".to_string()
    } else {
        "LOW".to_string()
    }
}

fn build_text_field(
    value: String,
    confidence: f64,
    evidences: Vec<String>,
) -> Option<ResumeProfileFieldText> {
    let normalized = value.trim().to_string();
    if normalized.is_empty() {
        return None;
    }
    Some(ResumeProfileFieldText {
        value: normalized,
        confidence,
        confidence_level: confidence_level(confidence),
        evidences,
    })
}

fn build_number_field(
    value: f64,
    confidence: f64,
    evidences: Vec<String>,
) -> Option<ResumeProfileFieldNumber> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    Some(ResumeProfileFieldNumber {
        value,
        confidence,
        confidence_level: confidence_level(confidence),
        evidences,
    })
}

fn build_int_field(
    value: i32,
    confidence: f64,
    evidences: Vec<String>,
) -> Option<ResumeProfileFieldInt> {
    if value < 0 {
        return None;
    }
    Some(ResumeProfileFieldInt {
        value,
        confidence,
        confidence_level: confidence_level(confidence),
        evidences,
    })
}

fn extract_field_line_with_regex(lines: &[&str], regex: &Regex) -> Option<(String, String)> {
    for line in lines {
        if let Some(capture) = regex.captures(line) {
            let Some(value) = capture.get(1).map(|item| item.as_str().trim()) else {
                continue;
            };
            if value.is_empty() {
                continue;
            }
            return Some((value.to_string(), (*line).trim().to_string()));
        }
    }
    None
}

pub(crate) fn extract_resume_profile_fields(raw_text: &str) -> ResumeProfilePreviewExtracted {
    let normalized = normalize_resume_text(raw_text);
    let lines = normalized
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    let name = {
        let explicit = Regex::new(r"(?i)(?:姓名|name)\s*[:：]\s*([^\s|,，]{2,20})").ok();
        if let Some(regex) = explicit.as_ref() {
            if let Some((value, evidence)) = extract_field_line_with_regex(&lines, regex) {
                build_text_field(value, 0.95, vec![evidence])
            } else {
                let first_line = lines.first().copied().unwrap_or_default();
                let looks_like_name = first_line.chars().count() <= 16
                    && !first_line.contains('@')
                    && !first_line.contains("简历")
                    && !first_line.contains("工作")
                    && !first_line.contains("经验");
                if looks_like_name {
                    build_text_field(first_line.to_string(), 0.62, vec![first_line.to_string()])
                } else {
                    None
                }
            }
        } else {
            None
        }
    };

    let current_company = {
        let regex = Regex::new(
            r"(?i)(?:当前公司|现公司|目前公司|现任公司|所在公司|就职于|任职于|current\s*company)\s*[:：]?\s*([^\n,，|]{2,60})",
        )
        .ok();
        if let Some(regex) = regex.as_ref() {
            if let Some((value, evidence)) = extract_field_line_with_regex(&lines, regex) {
                build_text_field(value, 0.88, vec![evidence])
            } else {
                None
            }
        } else {
            None
        }
    };

    let years_of_experience = {
        let regexes = [
            Regex::new(r"(?i)(\d{1,2}(?:\.\d+)?)\s*年(?:工作)?经验").ok(),
            Regex::new(r"(?i)工作年限\s*[:：]?\s*(\d{1,2}(?:\.\d+)?)").ok(),
            Regex::new(r"(?i)experience\s*[:：]?\s*(\d{1,2}(?:\.\d+)?)").ok(),
        ];
        let mut best_value: Option<f64> = None;
        let mut evidences = Vec::<String>::new();
        let mut best_confidence = 0.0_f64;

        for regex in regexes.iter().flatten() {
            for line in lines.iter().copied() {
                let Some(capture) = regex.captures(line) else {
                    continue;
                };
                let Some(raw_number) = capture.get(1).map(|item| item.as_str()) else {
                    continue;
                };
                let Ok(parsed) = raw_number.parse::<f64>() else {
                    continue;
                };
                if !(0.0..=50.0).contains(&parsed) {
                    continue;
                }
                if best_value.is_none() || parsed > best_value.unwrap_or(0.0) {
                    best_value = Some(parsed);
                    evidences = vec![line.to_string()];
                    best_confidence = 0.9;
                }
            }
        }

        best_value.and_then(|value| build_number_field(value, best_confidence, evidences))
    };

    let age = {
        let regexes = [
            Regex::new(r"(?i)年龄\s*[:：]?\s*(\d{2})").ok(),
            Regex::new(r"(?i)(\d{2})\s*岁").ok(),
        ];
        let mut matched: Option<i32> = None;
        let mut evidence = String::new();
        let mut confidence = 0.0_f64;
        for regex in regexes.iter().flatten() {
            for line in lines.iter().copied() {
                let Some(capture) = regex.captures(line) else {
                    continue;
                };
                let Some(raw_number) = capture.get(1).map(|item| item.as_str()) else {
                    continue;
                };
                let Ok(parsed) = raw_number.parse::<i32>() else {
                    continue;
                };
                if !(16..=70).contains(&parsed) {
                    continue;
                }
                matched = Some(parsed);
                evidence = line.to_string();
                confidence = if line.contains("年龄") { 0.92 } else { 0.72 };
                break;
            }
            if matched.is_some() {
                break;
            }
        }
        matched.and_then(|value| build_int_field(value, confidence, vec![evidence]))
    };

    let gender = {
        let regex = Regex::new(r"(?i)性别\s*[:：]?\s*(男|女|male|female)").ok();
        if let Some(regex) = regex.as_ref() {
            if let Some((value, evidence)) = extract_field_line_with_regex(&lines, regex) {
                let normalized_gender = match value.to_lowercase().as_str() {
                    "男" | "male" => "male",
                    "女" | "female" => "female",
                    _ => "other",
                }
                .to_string();
                build_text_field(normalized_gender, 0.94, vec![evidence])
            } else {
                None
            }
        } else {
            None
        }
    };

    let address = {
        let regex = Regex::new(
            r"(?i)(?:现居住地|居住地|住址|地址|所在地|location|现居)\s*[:：]?\s*([^\n]{2,80})",
        )
        .ok();
        if let Some(regex) = regex.as_ref() {
            if let Some((value, evidence)) = extract_field_line_with_regex(&lines, regex) {
                build_text_field(value, 0.83, vec![evidence])
            } else {
                None
            }
        } else {
            None
        }
    };

    let phone = {
        let regex = Regex::new(r"(?i)(\+?\d[\d\-\s]{6,}\d)").ok();
        if let Some(regex) = regex.as_ref() {
            if let Some((value, evidence)) = extract_field_line_with_regex(&lines, regex) {
                let normalized = value
                    .chars()
                    .filter(|ch| ch.is_ascii_digit() || *ch == '+')
                    .collect::<String>();
                build_text_field(normalized, 0.9, vec![evidence])
            } else {
                None
            }
        } else {
            None
        }
    };

    let email = {
        let regex = Regex::new(r"(?i)([a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,})").ok();
        if let Some(regex) = regex.as_ref() {
            if let Some((value, evidence)) = extract_field_line_with_regex(&lines, regex) {
                build_text_field(value.to_lowercase(), 0.97, vec![evidence])
            } else {
                None
            }
        } else {
            None
        }
    };

    ResumeProfilePreviewExtracted {
        name,
        current_company,
        years_of_experience,
        age,
        gender,
        address,
        phone,
        email,
    }
}
