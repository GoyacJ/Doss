import type { CrawlTaskStatus, InterviewRecommendation, PipelineStage } from "@doss/shared";

export type StatusTone = "neutral" | "info" | "success" | "warning" | "danger";

export function stageTone(stage: PipelineStage): StatusTone {
  if (stage === "OFFERED") {
    return "success";
  }
  if (stage === "REJECTED") {
    return "danger";
  }
  if (stage === "HOLD") {
    return "warning";
  }
  if (stage === "SCREENING" || stage === "INTERVIEW") {
    return "info";
  }
  return "neutral";
}

export function taskStatusLabel(status: CrawlTaskStatus): string {
  if (status === "PENDING") {
    return "待执行";
  }
  if (status === "RUNNING") {
    return "进行中";
  }
  if (status === "PAUSED") {
    return "已暂停";
  }
  if (status === "CANCELED") {
    return "已取消";
  }
  if (status === "SUCCEEDED") {
    return "成功";
  }
  return "失败";
}

export function taskStatusTone(status: CrawlTaskStatus): StatusTone {
  if (status === "SUCCEEDED") {
    return "success";
  }
  if (status === "FAILED") {
    return "danger";
  }
  if (status === "PAUSED" || status === "CANCELED") {
    return "warning";
  }
  if (status === "RUNNING") {
    return "info";
  }
  return "neutral";
}

export function sidecarHealthBadge(healthy: boolean | null): {
  label: string;
  tone: StatusTone;
} {
  if (healthy === true) {
    return { label: "在线", tone: "success" };
  }
  if (healthy === false) {
    return { label: "离线", tone: "danger" };
  }
  return { label: "未知", tone: "neutral" };
}

export function interviewRecommendationLabel(recommendation: InterviewRecommendation): string {
  if (recommendation === "HIRE") {
    return "建议录用";
  }
  if (recommendation === "HOLD") {
    return "建议待定";
  }
  return "建议不录用";
}

export function interviewRecommendationTone(recommendation: InterviewRecommendation): StatusTone {
  if (recommendation === "HIRE") {
    return "success";
  }
  if (recommendation === "HOLD") {
    return "warning";
  }
  return "danger";
}
