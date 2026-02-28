import { resolveScoringRerunFeedback, type ScoringRerunFeedback } from "./scoring-rerun-feedback";

export type ScreeningRerunFeedback = ScoringRerunFeedback;

export function resolveScreeningRerunFeedback(
  error: unknown,
  fallback = "重新分析失败",
): ScoringRerunFeedback {
  return resolveScoringRerunFeedback(error, fallback);
}
