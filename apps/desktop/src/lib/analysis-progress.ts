export const ANALYSIS_PROGRESS_EVENT = "candidate-ai-analysis-progress";

export type AnalysisProgressPhase = "prepare" | "ai" | "t0" | "t1" | "t2" | "t3" | "persist";
export type AnalysisProgressStatus = "running" | "completed" | "failed";
export type AnalysisProgressKind = "start" | "progress" | "retry" | "summary" | "end";

export interface AnalysisProgressEventPayload {
  runId: string;
  candidateId: number;
  phase: AnalysisProgressPhase;
  status: AnalysisProgressStatus;
  kind: AnalysisProgressKind;
  message: string;
  meta?: Record<string, unknown>;
  at: string;
}

export interface AnalysisTraceItem extends AnalysisProgressEventPayload {
  id: string;
}

const FINAL_STEP_INDEX = 5;

function toTimestamp(value: string): number {
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

function pad2(value: number): string {
  return String(value).padStart(2, "0");
}

export function formatAnalysisTraceElapsed(at: string, startedAtMs: number): string {
  const eventAt = Date.parse(at);
  if (!Number.isFinite(eventAt)) {
    return at;
  }
  const base = startedAtMs > 0 ? startedAtMs : eventAt;
  const seconds = Math.max(0, Math.floor((eventAt - base) / 1000));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const restSeconds = seconds % 60;
  if (hours > 0) {
    return `T+${pad2(hours)}:${pad2(minutes)}:${pad2(restSeconds)}`;
  }
  return `T+${pad2(minutes)}:${pad2(restSeconds)}`;
}

export function phaseToStepIndex(phase: AnalysisProgressPhase): number {
  if (phase === "prepare") {
    return 0;
  }
  if (phase === "ai" || phase === "t0") {
    return 1;
  }
  if (phase === "t1") {
    return 2;
  }
  if (phase === "t2") {
    return 3;
  }
  if (phase === "t3") {
    return 4;
  }
  return FINAL_STEP_INDEX;
}

export function shouldAcceptAnalysisProgressEvent(
  payload: AnalysisProgressEventPayload,
  runId: string,
  candidateId: number,
): boolean {
  if (!runId || payload.runId !== runId) {
    return false;
  }
  return payload.candidateId === candidateId;
}

export function resolveAnalysisStepIndex(
  currentIndex: number,
  phase: AnalysisProgressPhase,
  status: AnalysisProgressStatus,
): number {
  const phaseIndex = phaseToStepIndex(phase);
  if (phaseIndex < 0) {
    return currentIndex;
  }
  if (status === "completed" && phase === "persist") {
    return currentIndex <= 2 ? 2 : FINAL_STEP_INDEX;
  }
  return Math.max(currentIndex, phaseIndex);
}

export function appendAnalysisTrace(
  items: AnalysisTraceItem[],
  payload: AnalysisProgressEventPayload,
  maxItems = 30,
): AnalysisTraceItem[] {
  const withId: AnalysisTraceItem = {
    id: `${payload.runId}-${payload.phase}-${payload.kind}-${payload.at}-${items.length}`,
    ...payload,
  };

  const sorted = [...items, withId].sort((left, right) => {
    const delta = toTimestamp(left.at) - toTimestamp(right.at);
    if (delta !== 0) {
      return delta;
    }
    return left.id.localeCompare(right.id);
  });

  if (sorted.length <= maxItems) {
    return sorted;
  }
  return sorted.slice(sorted.length - maxItems);
}

export function buildFallbackAnalysisMessage(phase: AnalysisProgressPhase): string {
  if (phase === "prepare") {
    return "正在解析候选人与模板上下文...";
  }
  if (phase === "t0") {
    return "正在评估 T0 重要指标...";
  }
  if (phase === "ai") {
    return "正在评估候选人与岗位匹配度...";
  }
  if (phase === "t1") {
    return "正在评估 T1 指标配置...";
  }
  if (phase === "t2") {
    return "正在评估 T2 加分项...";
  }
  if (phase === "t3") {
    return "正在评估 T3 风险项...";
  }
  return "正在写入评分结果并刷新视图...";
}
