export type ScoringRerunFeedback = {
  tone: "warning" | "danger";
  message: string;
};

const RESUME_REQUIRED_BEFORE_SCORING = "Resume required before scoring";
const RESUME_REQUIRED_BEFORE_ANALYSIS = "Resume required before analysis";
const RESUME_REQUIRED_BEFORE_SCREENING = "Resume required before screening";

function resolveErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  if (typeof error === "string" && error.trim()) {
    return error;
  }
  return fallback;
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
  if (message.startsWith("scoring_task_join_error")) {
    return {
      tone: "danger",
      message: "评分任务超时或中断，请稍后重试",
    };
  }
  return {
    tone: "danger",
    message,
  };
}
