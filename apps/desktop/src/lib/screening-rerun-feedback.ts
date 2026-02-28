export type ScreeningRerunFeedback = {
  tone: "warning" | "danger";
  message: string;
};

const RESUME_REQUIRED_BEFORE_SCREENING = "Resume required before screening";
const RESUME_REQUIRED_BEFORE_ANALYSIS = "Resume required before analysis";

function resolveErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  if (typeof error === "string" && error.trim()) {
    return error;
  }
  return fallback;
}

export function resolveScreeningRerunFeedback(
  error: unknown,
  fallback = "重新分析失败",
): ScreeningRerunFeedback {
  const message = resolveErrorMessage(error, fallback);
  if (message === RESUME_REQUIRED_BEFORE_SCREENING || message === RESUME_REQUIRED_BEFORE_ANALYSIS) {
    return {
      tone: "warning",
      message: "请先上传简历后再重新分析",
    };
  }
  return {
    tone: "danger",
    message,
  };
}
