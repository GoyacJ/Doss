import type { ScreeningResultRecord } from "../services/backend";

export interface StructuredTemplateViewItem {
  key: string;
  label: string;
  weight: number;
  score5: number | null;
  score100: number | null;
  comment: string;
}

export interface StructuredBonusViewItem {
  title: string;
  score5: number | null;
  comment: string;
}

export interface StructuredRiskViewItem {
  title: string;
  severity: string;
  comment: string;
}

export interface StructuredScreeningViewModel {
  overallScore5: number | null;
  overallScore100: number | null;
  overallComment: string;
  recommendation: string;
  riskLevel: string;
  weights: { t0: number; t1: number; t2: number; t3: number };
  subscores: { t0: number | null; t1: number | null; t2: number | null; t3: number | null };
  templateName: string;
  templateItems: StructuredTemplateViewItem[];
  bonusScore5: number | null;
  bonusItems: StructuredBonusViewItem[];
  bonusComment: string;
  riskScore5: number | null;
  riskItems: StructuredRiskViewItem[];
  riskComment: string;
  riskAlerts: string[];
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

function asArray(value: unknown): unknown[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value;
}

function asString(value: unknown): string {
  if (typeof value === "string") {
    return value.trim();
  }
  return "";
}

function asNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string") {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) {
      return parsed;
    }
  }
  return null;
}

function round(value: number, digits: number): number {
  const unit = 10 ** digits;
  return Math.round(value * unit) / unit;
}

function toScore5From100(score100: number | null | undefined): number | null {
  if (score100 === null || score100 === undefined || !Number.isFinite(score100)) {
    return null;
  }
  return round((score100 / 20), 2);
}

function riskLevelScore5(level: string): number {
  if (level === "HIGH") {
    return 1;
  }
  if (level === "MEDIUM") {
    return 3;
  }
  return 5;
}

function normalizeTemplateItem(item: unknown): StructuredTemplateViewItem | null {
  const record = asRecord(item);
  if (!record) {
    return null;
  }

  const key = asString(record.key);
  const label = asString(record.label);
  const weight = asNumber(record.weight);
  const score5 = asNumber(record.score_5) ?? asNumber(record.score5);
  const score100 = asNumber(record.score_100) ?? asNumber(record.score100) ?? asNumber(record.score);
  const comment = asString(record.comment) || asString(record.reason);

  if (!key || !label || weight === null) {
    return null;
  }

  return {
    key,
    label,
    weight,
    score5: score5 === null ? toScore5From100(score100) : round(score5, 2),
    score100: score100 === null ? null : Math.round(score100),
    comment: comment || "暂无评价",
  };
}

function normalizeBonusItem(item: unknown): StructuredBonusViewItem | null {
  if (typeof item === "string" && item.trim()) {
    return {
      title: item.trim(),
      score5: null,
      comment: "来源于历史结构化结果",
    };
  }
  const record = asRecord(item);
  if (!record) {
    return null;
  }

  const title = asString(record.title);
  const score5 = asNumber(record.score_5) ?? asNumber(record.score5);
  const comment = asString(record.comment);
  if (!title) {
    return null;
  }

  return {
    title,
    score5: score5 === null ? null : round(score5, 2),
    comment: comment || "暂无评价",
  };
}

function normalizeRiskItem(item: unknown): StructuredRiskViewItem | null {
  if (typeof item === "string" && item.trim()) {
    return {
      title: item.trim(),
      severity: "MEDIUM",
      comment: "来源于历史结构化结果",
    };
  }
  const record = asRecord(item);
  if (!record) {
    return null;
  }
  const title = asString(record.title);
  const severity = asString(record.severity).toUpperCase() || "MEDIUM";
  const comment = asString(record.comment);
  if (!title) {
    return null;
  }
  return {
    title,
    severity,
    comment: comment || "暂无评价",
  };
}

export function resolveStructuredScreeningViewModel(
  screening: ScreeningResultRecord | null | undefined,
): StructuredScreeningViewModel | null {
  if (!screening) {
    return null;
  }

  const structured = asRecord(screening.structured_result) ?? {};
  const summary = asRecord(structured.summary);
  const templateAssessment = asRecord(structured.template_assessment);
  const bonusAssessment = asRecord(structured.bonus_assessment);
  const riskAssessment = asRecord(structured.risk_assessment);

  const fallbackWeights = { t0: 50, t1: 30, t2: 10, t3: 10 };
  const summaryWeights = asRecord(summary?.weights);
  const summarySubscores = asRecord(summary?.subscores);

  const overallScore100 = asNumber(summary?.overall_score_100)
    ?? asNumber(summary?.overallScore100)
    ?? screening.overall_score;
  const overallScore5 = asNumber(summary?.overall_score_5)
    ?? asNumber(summary?.overallScore5)
    ?? toScore5From100(overallScore100);

  const t0Score = asNumber(summarySubscores?.t0) ?? screening.t0_score;
  const t1Score = asNumber(summarySubscores?.t1) ?? toScore5From100(screening.t1_score);
  const t2Score = asNumber(summarySubscores?.t2)
    ?? (screening.bonus_score ? round((screening.bonus_score / 15) * 5, 2) : 0);
  const t3Score = asNumber(summarySubscores?.t3) ?? riskLevelScore5(screening.risk_level);

  const fallbackWeightsOld = asRecord(structured.weights);
  const t1Old = asRecord(fallbackWeightsOld?.t1);
  const t2Old = asRecord(fallbackWeightsOld?.t2);
  const templateItemsFromV2 = asArray(templateAssessment?.items)
    .map(normalizeTemplateItem)
    .filter((item): item is StructuredTemplateViewItem => Boolean(item));
  const templateItemsFromV1 = asArray(t1Old?.items)
    .map(normalizeTemplateItem)
    .filter((item): item is StructuredTemplateViewItem => Boolean(item));
  const templateItems = templateItemsFromV2.length ? templateItemsFromV2 : templateItemsFromV1;

  const bonusItems = asArray(bonusAssessment?.items ?? t2Old?.items)
    .map(normalizeBonusItem)
    .filter((item): item is StructuredBonusViewItem => Boolean(item));

  const rawRiskAlerts = asArray(structured.risk_alerts)
    .filter((item): item is string => typeof item === "string")
    .map((item) => item.trim())
    .filter(Boolean);
  const riskItems = asArray(riskAssessment?.items ?? structured.risk_items ?? rawRiskAlerts)
    .map(normalizeRiskItem)
    .filter((item): item is StructuredRiskViewItem => Boolean(item));

  const overallComment = asString(summary?.overall_comment)
    || asString(summary?.overallComment)
    || asString(structured.overall_comment)
    || screening.recommendation;

  const templateName = asString(templateAssessment?.template)
    || asString(t1Old?.template)
    || "默认筛选模板";

  const bonusScore5 = asNumber(bonusAssessment?.score_5)
    ?? asNumber(bonusAssessment?.score5)
    ?? (() => {
      const legacyBonus = asNumber(t2Old?.bonus) ?? screening.bonus_score;
      return round((legacyBonus / 15) * 5, 2);
    })();

  const riskLevel = asString(riskAssessment?.level) || screening.risk_level;
  const riskScore5 = asNumber(riskAssessment?.score_5)
    ?? asNumber(riskAssessment?.score5)
    ?? riskLevelScore5(riskLevel);

  const weights = {
    t0: asNumber(summaryWeights?.t0) ?? fallbackWeights.t0,
    t1: asNumber(summaryWeights?.t1) ?? fallbackWeights.t1,
    t2: asNumber(summaryWeights?.t2) ?? fallbackWeights.t2,
    t3: asNumber(summaryWeights?.t3) ?? fallbackWeights.t3,
  };

  return {
    overallScore5: overallScore5 === null ? null : round(overallScore5, 2),
    overallScore100: overallScore100 === null ? null : Math.round(overallScore100),
    overallComment: overallComment || "-",
    recommendation: screening.recommendation,
    riskLevel,
    weights,
    subscores: {
      t0: t0Score === null ? null : round(t0Score, 2),
      t1: t1Score === null ? null : round(t1Score, 2),
      t2: t2Score === null ? null : round(t2Score, 2),
      t3: t3Score === null ? null : round(t3Score, 2),
    },
    templateName,
    templateItems,
    bonusScore5: bonusScore5 === null ? null : round(bonusScore5, 2),
    bonusItems,
    bonusComment: asString(bonusAssessment?.comment) || "暂无加分项评价",
    riskScore5: riskScore5 === null ? null : round(riskScore5, 2),
    riskItems,
    riskComment: asString(riskAssessment?.comment) || "暂无风险评估说明",
    riskAlerts: rawRiskAlerts,
  };
}

export function riskSeverityTone(severity: string): "danger" | "warning" | "info" {
  const normalized = severity.trim().toUpperCase();
  if (normalized === "HIGH") {
    return "danger";
  }
  if (normalized === "LOW") {
    return "info";
  }
  return "warning";
}
