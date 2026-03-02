import type { PipelineStage } from "@doss/shared";

const LABELS: Record<PipelineStage, string> = {
  NEW: "待筛选",
  SCREENING: "初筛",
  INTERVIEW: "面试",
  HOLD: "待定",
  REJECTED: "淘汰",
  OFFERED: "录用",
};

const NEXT_STAGE_MAP: Record<PipelineStage, PipelineStage[]> = {
  NEW: ["NEW", "SCREENING", "INTERVIEW", "HOLD", "REJECTED", "OFFERED"],
  SCREENING: ["NEW", "SCREENING", "INTERVIEW", "HOLD", "REJECTED", "OFFERED"],
  INTERVIEW: ["NEW", "SCREENING", "INTERVIEW", "HOLD", "REJECTED", "OFFERED"],
  HOLD: ["NEW", "SCREENING", "INTERVIEW", "HOLD", "REJECTED", "OFFERED"],
  REJECTED: ["NEW", "SCREENING", "INTERVIEW", "HOLD", "REJECTED", "OFFERED"],
  OFFERED: ["NEW", "SCREENING", "INTERVIEW", "HOLD", "REJECTED", "OFFERED"],
};

export function formatStageLabel(stage: PipelineStage): string {
  return LABELS[stage];
}

export function nextStageOptions(stage: PipelineStage): PipelineStage[] {
  return NEXT_STAGE_MAP[stage];
}
