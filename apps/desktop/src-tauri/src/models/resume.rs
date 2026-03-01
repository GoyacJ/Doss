use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) const RESUME_SCHEMA_VERSION_V2: i32 = 2;
pub(crate) const RESUME_PARSER_VERSION: &str = "resume-parser-v3";

fn default_resume_content_format() -> String {
    "plain".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub(crate) struct ResumeParseMeta {
    pub(crate) parser_version: String,
    pub(crate) parsed_at: String,
    pub(crate) source: String,
    pub(crate) ocr_used: bool,
    pub(crate) text_length: usize,
    pub(crate) section_count: usize,
    #[serde(default = "default_resume_content_format")]
    pub(crate) content_format: String,
    pub(crate) source_extension: Option<String>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct ResumeBasicInfo {
    pub(crate) name: Option<String>,
    pub(crate) email: Option<String>,
    pub(crate) phone: Option<String>,
    pub(crate) location: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) summary: Option<String>,
    pub(crate) years_of_experience: Option<f64>,
    pub(crate) expected_salary_k: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct ResumeEducationItem {
    pub(crate) school: Option<String>,
    pub(crate) degree: Option<String>,
    pub(crate) major: Option<String>,
    pub(crate) start: Option<String>,
    pub(crate) end: Option<String>,
    pub(crate) description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct ResumeWorkExperienceItem {
    pub(crate) company: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) start: Option<String>,
    pub(crate) end: Option<String>,
    pub(crate) summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct ResumeProjectItem {
    pub(crate) name: Option<String>,
    pub(crate) role: Option<String>,
    pub(crate) start: Option<String>,
    pub(crate) end: Option<String>,
    pub(crate) summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct ResumeCertificateItem {
    pub(crate) name: String,
    pub(crate) issuer: Option<String>,
    pub(crate) date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct ResumeLanguageItem {
    pub(crate) name: String,
    pub(crate) level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct ResumeSection {
    pub(crate) key: String,
    pub(crate) title: String,
    pub(crate) content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct ResumeDerivedMetrics {
    pub(crate) project_count: usize,
    pub(crate) work_experience_count: usize,
    pub(crate) education_count: usize,
    pub(crate) skill_count: usize,
    pub(crate) section_count: usize,
    pub(crate) text_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub(crate) struct ResumeParsedV2 {
    pub(crate) schema_version: i32,
    pub(crate) parse_meta: ResumeParseMeta,
    pub(crate) basic: ResumeBasicInfo,
    pub(crate) skills: Vec<String>,
    pub(crate) education: Vec<ResumeEducationItem>,
    pub(crate) work_experiences: Vec<ResumeWorkExperienceItem>,
    pub(crate) projects: Vec<ResumeProjectItem>,
    pub(crate) certificates: Vec<ResumeCertificateItem>,
    pub(crate) languages: Vec<ResumeLanguageItem>,
    pub(crate) sections: Vec<ResumeSection>,
    pub(crate) derived_metrics: ResumeDerivedMetrics,
}

impl ResumeParsedV2 {
    pub(crate) fn is_v2_json(value: &Value) -> bool {
        value
            .get("schema_version")
            .and_then(|item| item.as_i64())
            .map(|version| version as i32 == RESUME_SCHEMA_VERSION_V2)
            .unwrap_or(false)
    }

    pub(crate) fn from_value(value: Value) -> Result<Self, String> {
        let mut parsed =
            serde_json::from_value::<ResumeParsedV2>(value).map_err(|error| error.to_string())?;
        parsed.schema_version = RESUME_SCHEMA_VERSION_V2;
        Ok(parsed)
    }

    pub(crate) fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| serde_json::json!({}))
    }
}
