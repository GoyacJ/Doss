use serde::{Deserialize, Serialize};

use crate::core::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum PipelineStage {
    New,
    Screening,
    Interview,
    Hold,
    Rejected,
    Offered,
}

impl PipelineStage {
    pub(crate) fn as_db(&self) -> &'static str {
        match self {
            PipelineStage::New => "NEW",
            PipelineStage::Screening => "SCREENING",
            PipelineStage::Interview => "INTERVIEW",
            PipelineStage::Hold => "HOLD",
            PipelineStage::Rejected => "REJECTED",
            PipelineStage::Offered => "OFFERED",
        }
    }

    pub(crate) fn from_db(value: &str) -> AppResult<Self> {
        match value {
            "NEW" => Ok(PipelineStage::New),
            "SCREENING" => Ok(PipelineStage::Screening),
            "INTERVIEW" => Ok(PipelineStage::Interview),
            "HOLD" => Ok(PipelineStage::Hold),
            "REJECTED" => Ok(PipelineStage::Rejected),
            "OFFERED" => Ok(PipelineStage::Offered),
            _ => Err(AppError::NotFound(format!("Unknown stage: {value}"))),
        }
    }
}

pub(crate) fn is_valid_transition(from: &str, to: &str) -> bool {
    match (from, to) {
        ("NEW", "SCREENING")
        | ("NEW", "REJECTED")
        | ("SCREENING", "INTERVIEW")
        | ("SCREENING", "HOLD")
        | ("SCREENING", "REJECTED")
        | ("INTERVIEW", "OFFERED")
        | ("INTERVIEW", "HOLD")
        | ("INTERVIEW", "REJECTED") => true,
        _ => from == to,
    }
}

pub(crate) fn resolve_qualification_stage(
    current_stage: &str,
    qualified: bool,
) -> Option<&'static str> {
    if qualified {
        if current_stage == "REJECTED" {
            Some("NEW")
        } else {
            None
        }
    } else if current_stage == "REJECTED" {
        None
    } else {
        Some("REJECTED")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum CrawlTaskStatus {
    Pending,
    Running,
    Paused,
    Canceled,
    Succeeded,
    Failed,
}

impl CrawlTaskStatus {
    pub(crate) fn as_db(&self) -> &'static str {
        match self {
            CrawlTaskStatus::Pending => "PENDING",
            CrawlTaskStatus::Running => "RUNNING",
            CrawlTaskStatus::Paused => "PAUSED",
            CrawlTaskStatus::Canceled => "CANCELED",
            CrawlTaskStatus::Succeeded => "SUCCEEDED",
            CrawlTaskStatus::Failed => "FAILED",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SourceType {
    Boss,
    Zhilian,
    Wuba,
    Lagou,
    All,
    Manual,
}

impl SourceType {
    pub(crate) fn as_db(&self) -> &'static str {
        match self {
            SourceType::Boss => "boss",
            SourceType::Zhilian => "zhilian",
            SourceType::Wuba => "wuba",
            SourceType::Lagou => "lagou",
            SourceType::All => "all",
            SourceType::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CrawlMode {
    Compliant,
    Advanced,
}

impl CrawlMode {
    pub(crate) fn as_db(&self) -> &'static str {
        match self {
            CrawlMode::Compliant => "compliant",
            CrawlMode::Advanced => "advanced",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AiProvider {
    Qwen,
    Doubao,
    Deepseek,
    Minimax,
    Glm,
    OpenApi,
}

impl AiProvider {
    pub(crate) fn all() -> [AiProvider; 6] {
        [
            AiProvider::Qwen,
            AiProvider::Doubao,
            AiProvider::Deepseek,
            AiProvider::Minimax,
            AiProvider::Glm,
            AiProvider::OpenApi,
        ]
    }

    pub(crate) fn as_db(&self) -> &'static str {
        match self {
            AiProvider::Qwen => "qwen",
            AiProvider::Doubao => "doubao",
            AiProvider::Deepseek => "deepseek",
            AiProvider::Minimax => "minimax",
            AiProvider::Glm => "glm",
            AiProvider::OpenApi => "openapi",
        }
    }

    pub(crate) fn from_db(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "qwen" => AiProvider::Qwen,
            "doubao" => AiProvider::Doubao,
            "deepseek" => AiProvider::Deepseek,
            "minimax" => AiProvider::Minimax,
            "glm" => AiProvider::Glm,
            "openapi" | "open-api" | "openapi_compatible" | "openapi-compatible"
            | "openai_compatible" | "openai-compatible" | "openai" => AiProvider::OpenApi,
            "mock" => AiProvider::Qwen,
            _ => AiProvider::Qwen,
        }
    }

    pub(crate) fn default_model(&self) -> &'static str {
        match self {
            AiProvider::Qwen => "qwen-plus-latest",
            AiProvider::Doubao => "doubao-seed-1-6-250615",
            AiProvider::Deepseek => "deepseek-chat",
            AiProvider::Minimax => "MiniMax-M2.5",
            AiProvider::Glm => "glm-5-air",
            AiProvider::OpenApi => "gpt-4.1-mini",
        }
    }

    pub(crate) fn default_base_url(&self) -> &'static str {
        match self {
            AiProvider::Qwen => "https://dashscope.aliyuncs.com/compatible-mode/v1",
            AiProvider::Doubao => "https://ark.cn-beijing.volces.com/api/v3",
            AiProvider::Deepseek => "https://api.deepseek.com",
            AiProvider::Minimax => "https://api.minimaxi.com/v1",
            AiProvider::Glm => "https://open.bigmodel.cn/api/paas/v4",
            AiProvider::OpenApi => "https://api.openai.com/v1",
        }
    }

    pub(crate) fn label(&self) -> &'static str {
        match self {
            AiProvider::Qwen => "千问 Qwen",
            AiProvider::Doubao => "豆包 Doubao",
            AiProvider::Deepseek => "DeepSeek",
            AiProvider::Minimax => "MiniMax",
            AiProvider::Glm => "GLM",
            AiProvider::OpenApi => "OpenApi",
        }
    }

    pub(crate) fn models(&self) -> &'static [&'static str] {
        match self {
            AiProvider::Qwen => &[
                "qwen3-max-preview",
                "qwen3-max-preview-thinking",
                "qwen3-max",
                "qwen-plus-latest",
                "qwen-plus",
                "qwen-turbo-latest",
                "qwen-turbo",
                "qwen-flash-latest",
                "qwen-flash",
                "qwen-long",
            ],
            AiProvider::Doubao => &[
                "doubao-seed-1-6-250615",
                "doubao-seed-1-6-thinking-250715",
                "doubao-seed-1-6-flash-250715",
            ],
            AiProvider::Deepseek => &["deepseek-chat", "deepseek-reasoner"],
            AiProvider::Minimax => &[
                "MiniMax-M2.5",
                "MiniMax-M2.5-Preview",
                "MiniMax-M2.5-Flash",
                "MiniMax-M2.5-highspeed",
                "MiniMax-M2.1",
                "abab8.5-chat",
                "abab8.5s-chat",
            ],
            AiProvider::Glm => &[
                "glm-5",
                "glm-5-air",
                "glm-5-airx",
                "glm-5-flash",
                "glm-4.5",
                "glm-4.5-air",
            ],
            AiProvider::OpenApi => &[
                "gpt-5",
                "gpt-5-mini",
                "gpt-5-nano",
                "gpt-4.1",
                "gpt-4.1-mini",
                "gpt-4.1-nano",
                "o4-mini",
            ],
        }
    }

    pub(crate) fn docs(&self) -> &'static [&'static str] {
        match self {
            AiProvider::Qwen => &[
                "https://help.aliyun.com/zh/model-studio/developer-reference/compatibility-of-openai-with-dashscope",
                "https://help.aliyun.com/zh/model-studio/getting-started/models",
            ],
            AiProvider::Doubao => &[
                "https://www.volcengine.com/docs/82379/1541594",
                "https://www.volcengine.com/docs/63993/1573666",
            ],
            AiProvider::Deepseek => &[
                "https://api-docs.deepseek.com/",
                "https://api-docs.deepseek.com/quick_start/pricing",
            ],
            AiProvider::Minimax => &[
                "https://platform.minimaxi.com/document/Quick%20Start",
                "https://platform.minimaxi.com/document/Compatibility%20with%20OpenAI",
            ],
            AiProvider::Glm => &[
                "https://docs.bigmodel.cn/cn/guide/models/text/glm-5",
                "https://docs.bigmodel.cn/cn/guide/models",
            ],
            AiProvider::OpenApi => &[
                "https://platform.openai.com/docs/models",
                "https://platform.openai.com/docs/api-reference/chat/create",
            ],
        }
    }
}
