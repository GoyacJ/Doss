export const ANALYSIS_PROGRESS_EVENT = "candidate-analysis-progress";

export type AnalysisProgressPhase = "prepare" | "ai" | "persist";
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

const PHASE_ORDER: AnalysisProgressPhase[] = ["prepare", "ai", "persist"];

function toTimestamp(value: string): number {
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

export function phaseToStepIndex(phase: AnalysisProgressPhase): number {
  return PHASE_ORDER.indexOf(phase);
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
    return PHASE_ORDER.length - 1;
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
    return "正在整理候选人与岗位上下文...";
  }
  if (phase === "ai") {
    return "正在综合技能、年限与风险信号生成评估...";
  }
  return "正在写入分析结果并刷新视图...";
}

