export type ScoringRerunFeedback = {
  tone: "warning" | "danger";
  message: string;
};

const RESUME_REQUIRED_BEFORE_SCORING = "Resume required before scoring";
const RESUME_REQUIRED_BEFORE_ANALYSIS = "Resume required before analysis";
const RESUME_REQUIRED_BEFORE_SCREENING = "Resume required before screening";
const RESUME_FILE_REQUIRED_FOR_AI_ANALYSIS = "resume_file_required_for_ai_analysis";
const RESUME_FILE_TEXT_EMPTY_AFTER_PARSE = "resume_file_text_empty_after_parse";
const PROVIDER_RESPONSE_NOT_JSON_AFTER_REPAIR = "provider_response_not_json_after_repair";
const PROVIDER_RESPONSE_SCHEMA_INVALID = "provider_response_schema_invalid";
const CONTEXT_LIMIT_HINTS = [
  "context length",
  "maximum context",
  "max context",
  "too many tokens",
  "token limit",
  "prompt is too long",
  "input is too long",
  "context_window_exceeded",
  "context window exceeded",
];

function resolveErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  if (typeof error === "string" && error.trim()) {
    return error;
  }
  return fallback;
}

function isContextLimitError(message: string): boolean {
  const normalized = message.trim().toLowerCase();
  if (!normalized) {
    return false;
  }
  return CONTEXT_LIMIT_HINTS.some((hint) => normalized.includes(hint));
}

export function resolveScoringRerunFeedback(
  error: unknown,
  fallback = "重新分析失败",
): ScoringRerunFeedback {
  const message = resolveErrorMessage(error, fallback);
  if (
    message === RESUME_REQUIRED_BEFORE_SCORING
    || message === RESUME_REQUIRED_BEFORE_ANALYSIS
    || message === RESUME_REQUIRED_BEFORE_SCREENING
  ) {
    return {
      tone: "warning",
      message: "请先上传简历后再重新分析",
    };
  }
  if (message === RESUME_FILE_REQUIRED_FOR_AI_ANALYSIS) {
    return {
      tone: "warning",
      message: "请先上传简历文件后再重新分析",
    };
  }
  if (message === RESUME_FILE_TEXT_EMPTY_AFTER_PARSE) {
    return {
      tone: "warning",
      message: "简历文件解析为空，请检查文件内容或OCR设置",
    };
  }
  if (message.startsWith("scoring_task_join_error")) {
    return {
      tone: "danger",
      message: "评分任务超时或中断，请稍后重试",
    };
  }
  if (isContextLimitError(message)) {
    return {
      tone: "danger",
      message: "简历全文超过当前模型上下文上限，请切换长上下文模型后重试",
    };
  }
  if (message === PROVIDER_RESPONSE_NOT_JSON_AFTER_REPAIR) {
    return {
      tone: "danger",
      message: "模型返回格式异常，请稍后重试",
    };
  }
  if (message === PROVIDER_RESPONSE_SCHEMA_INVALID) {
    return {
      tone: "danger",
      message: "模型结果字段不完整，请稍后重试",
    };
  }
  return {
    tone: "danger",
    message,
  };
}
