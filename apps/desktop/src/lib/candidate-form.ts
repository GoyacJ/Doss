import type { CandidateGender } from "@doss/shared";
import type { NewCandidatePayload } from "../services/backend";

export interface CandidateManualDraft {
  name: string;
  currentCompany: string;
  jobId: number;
  score: number | string | null;
  age: number | string | null;
  gender: CandidateGender | "";
  yearsOfExperience: number | string;
  address: string;
  phone: string;
  email: string;
  tagsText: string;
}

type ManualCandidatePayload = Omit<NewCandidatePayload, "source" | "external_id">;

export type BuildCandidateManualPayloadResult =
  | { ok: true; payload: ManualCandidatePayload }
  | { ok: false; error: string };

function toOptionalText(value: string): string | undefined {
  const trimmed = value.trim();
  return trimmed || undefined;
}

function parseOptionalNumber(value: number | string | null | undefined): number | undefined {
  if (value === null || value === undefined) {
    return undefined;
  }

  if (typeof value === "number") {
    return Number.isFinite(value) ? value : Number.NaN;
  }

  const trimmed = value.trim();
  if (!trimmed) {
    return undefined;
  }
  const parsed = Number(trimmed);
  return Number.isFinite(parsed) ? parsed : Number.NaN;
}

export function normalizeCandidateTags(input: string): string[] {
  const dedup = new Map<string, string>();
  const segments = input
    .split(/[,\n\r;，；]+/g)
    .map((item) => item.trim())
    .filter(Boolean);

  for (const item of segments) {
    const key = item.toLowerCase();
    if (!dedup.has(key)) {
      dedup.set(key, item);
    }
  }

  return [...dedup.values()];
}

export function buildCandidateManualPayload(
  draft: CandidateManualDraft,
): BuildCandidateManualPayloadResult {
  const name = draft.name.trim();
  if (!name) {
    return { ok: false, error: "请填写候选人姓名" };
  }

  const years = parseOptionalNumber(draft.yearsOfExperience);
  if (years === undefined || Number.isNaN(years) || years < 0) {
    return { ok: false, error: "请填写有效工作年限" };
  }

  const score = parseOptionalNumber(draft.score);
  if (Number.isNaN(score)) {
    return { ok: false, error: "评分必须为数字" };
  }
  if (score !== undefined && (score < 0 || score > 100)) {
    return { ok: false, error: "评分范围需在 0-100 之间" };
  }

  const age = parseOptionalNumber(draft.age);
  if (Number.isNaN(age)) {
    return { ok: false, error: "年龄必须为数字" };
  }
  if (age !== undefined && age < 0) {
    return { ok: false, error: "年龄不能为负数" };
  }

  const payload: ManualCandidatePayload = {
    name,
    years_of_experience: years,
    tags: normalizeCandidateTags(draft.tagsText),
  };

  const currentCompany = toOptionalText(draft.currentCompany);
  const address = toOptionalText(draft.address);
  const phone = toOptionalText(draft.phone);
  const email = toOptionalText(draft.email);

  if (currentCompany) {
    payload.current_company = currentCompany;
  }
  if (draft.jobId > 0) {
    payload.job_id = draft.jobId;
  }
  if (score !== undefined) {
    payload.score = score;
  }
  if (age !== undefined) {
    payload.age = age;
  }
  if (draft.gender) {
    payload.gender = draft.gender;
  }
  if (address) {
    payload.address = address;
  }
  if (phone) {
    payload.phone = phone;
  }
  if (email) {
    payload.email = email;
  }

  return {
    ok: true,
    payload,
  };
}
