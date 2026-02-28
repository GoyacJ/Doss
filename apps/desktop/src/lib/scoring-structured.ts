import type { ScoringResultRecord } from "../services/backend";

export interface StructuredScoringSectionItem {
  key: string;
  label: string;
  description: string;
  weight: number;
  score5: number | null;
  reason: string;
  evidence: string;
}

export interface StructuredScoringSectionView {
  score5: number | null;
  comment: string;
  items: StructuredScoringSectionItem[];
}

export interface StructuredScoringViewModel {
  overallScore5: number | null;
  overallScore100: number | null;
  overallComment: string;
  recommendation: string;
  riskLevel: string;
  weights: { t0: number; t1: number; t2: number; t3: number };
  subscores: { t0: number | null; t1: number | null; t2: number | null; t3: number | null };
  templateName: string;
  sections: {
    t0: StructuredScoringSectionView;
    t1: StructuredScoringSectionView;
    t2: StructuredScoringSectionView;
    t3: StructuredScoringSectionView;
  };
  templateItems: Array<{
    key: string;
    label: string;
    weight: number;
    score5: number | null;
    score100: number | null;
    comment: string;
  }>;
  bonusScore5: number | null;
  bonusItems: Array<{
    title: string;
    score5: number | null;
    comment: string;
  }>;
  bonusComment: string;
  riskScore5: number | null;
  riskItems: Array<{
    title: string;
    severity: string;
    comment: string;
  }>;
  riskComment: string;
  riskAlerts: string[];
  highlights: string[];
  risks: string[];
  suggestions: string[];
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function asString(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
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

function round(value: number, digits = 2): number {
  const unit = 10 ** digits;
  return Math.round(value * unit) / unit;
}

function toScore100(score5: number | null): number | null {
  if (score5 === null) {
    return null;
  }
  return Math.round(score5 * 20);
}

function normalizeSectionItem(value: unknown): StructuredScoringSectionItem | null {
  const item = asRecord(value);
  if (!item) {
    return null;
  }

  const key = asString(item.key);
  const label = asString(item.label);
  const description = asString(item.description);
  const weight = asNumber(item.weight);
  const score5 = asNumber(item.score_5) ?? asNumber(item.score5);
  const reason = asString(item.reason);
  const evidence = asString(item.evidence);

  if (!key || !label || weight === null) {
    return null;
  }

  return {
    key,
    label,
    description,
    weight: Math.round(weight),
    score5: score5 === null ? null : round(score5),
    reason: reason || "暂无说明",
    evidence: evidence || "暂无证据",
  };
}

function emptySection(comment = "暂无区块评估结果"): StructuredScoringSectionView {
  return {
    score5: null,
    comment,
    items: [],
  };
}

function normalizeSection(value: unknown, fallbackComment: string): StructuredScoringSectionView {
  const section = asRecord(value);
  if (!section) {
    return emptySection(fallbackComment);
  }
  const items = asArray(section.items)
    .map(normalizeSectionItem)
    .filter((item): item is StructuredScoringSectionItem => Boolean(item));
  const score5 = asNumber(section.score_5) ?? asNumber(section.score5);
  const comment = asString(section.comment) || fallbackComment;
  return {
    score5: score5 === null ? null : round(score5),
    comment,
    items,
  };
}

function parseStringArray(value: unknown): string[] {
  return asArray(value)
    .filter((item): item is string => typeof item === "string")
    .map((item) => item.trim())
    .filter(Boolean);
}

export function resolveStructuredScoringViewModel(
  scoring: ScoringResultRecord | null | undefined,
): StructuredScoringViewModel | null {
  if (!scoring) {
    return null;
  }

  const structured = asRecord(scoring.structured_result) ?? {};
  const summary = asRecord(structured.summary);
  const templateAssessment = asRecord(structured.template_assessment);
  const weights = asRecord(summary?.weights);
  const subscores = asRecord(summary?.subscores);

  const t0 = normalizeSection(templateAssessment?.t0, "T0 重要指标暂无说明");
  const t1 = normalizeSection(templateAssessment?.t1, "T1 指标配置暂无说明");
  const t2 = normalizeSection(templateAssessment?.t2, "T2 加分项暂无说明");
  const t3 = normalizeSection(templateAssessment?.t3, "T3 风险项暂无说明");

  const riskLevel = asString(summary?.risk_level) || scoring.risk_level;
  const recommendation = asString(summary?.recommendation) || scoring.recommendation;

  return {
    overallScore5: round(asNumber(summary?.overall_score_5) ?? scoring.overall_score_5),
    overallScore100: Math.round(asNumber(summary?.overall_score_100) ?? scoring.overall_score),
    overallComment: asString(summary?.overall_comment) || "暂无综合结论",
    recommendation,
    riskLevel,
    weights: {
      t0: Math.round(asNumber(weights?.t0) ?? 50),
      t1: Math.round(asNumber(weights?.t1) ?? 30),
      t2: Math.round(asNumber(weights?.t2) ?? 10),
      t3: Math.round(asNumber(weights?.t3) ?? 10),
    },
    subscores: {
      t0: round(asNumber(subscores?.t0) ?? scoring.t0_score_5),
      t1: round(asNumber(subscores?.t1) ?? scoring.t1_score_5),
      t2: round(asNumber(subscores?.t2) ?? scoring.t2_score_5),
      t3: round(asNumber(subscores?.t3) ?? scoring.t3_score_5),
    },
    templateName: asString(templateAssessment?.template) || "默认评分模板",
    sections: { t0, t1, t2, t3 },
    templateItems: t1.items.map((item) => ({
      key: item.key,
      label: item.label,
      weight: item.weight,
      score5: item.score5,
      score100: toScore100(item.score5),
      comment: item.reason,
    })),
    bonusScore5: t2.score5,
    bonusItems: t2.items.map((item) => ({
      title: item.label,
      score5: item.score5,
      comment: item.reason,
    })),
    bonusComment: t2.comment,
    riskScore5: t3.score5,
    riskItems: t3.items.map((item) => ({
      title: item.label,
      severity: (item.score5 ?? 0) <= 2 ? "HIGH" : (item.score5 ?? 0) < 3.5 ? "MEDIUM" : "LOW",
      comment: item.reason,
    })),
    riskComment: t3.comment,
    riskAlerts: parseStringArray(structured.risks),
    highlights: parseStringArray(structured.highlights),
    risks: parseStringArray(structured.risks),
    suggestions: parseStringArray(structured.suggestions),
  };
}

export function riskSeverityTone(level: string): "danger" | "warning" | "info" {
  const normalized = level.toUpperCase();
  if (normalized === "HIGH") {
    return "danger";
  }
  if (normalized === "MEDIUM") {
    return "warning";
  }
  return "info";
}
