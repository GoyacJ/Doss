import crypto from "node:crypto";
import type { SourceType } from "@doss/shared";

type CrawlSource = Exclude<SourceType, "manual">;

interface RecordLike {
  [key: string]: unknown;
}

export interface NormalizedJobRow {
  externalId?: string;
  source: CrawlSource;
  title: string;
  company: string;
  city?: string;
  salaryK?: string;
  jobUrl?: string;
  description?: string;
  dedupeKey: string;
}

export interface NormalizedCandidateRow {
  externalId?: string;
  source: CrawlSource;
  name: string;
  currentCompany?: string;
  years: number;
  tag?: string;
  phone?: string;
  email?: string;
  dedupeKey: string;
}

export interface NormalizedResumePayload {
  rawText: string;
  parsed: Record<string, unknown>;
}

function asRecord(value: unknown): RecordLike | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return null;
  }

  return value as RecordLike;
}

function asString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function asNumber(value: unknown): number | undefined {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }

  if (typeof value === "string") {
    const parsed = Number.parseFloat(value);
    if (Number.isFinite(parsed)) {
      return parsed;
    }
  }

  return undefined;
}

function buildHash(seed: string): string {
  return crypto.createHash("sha1").update(seed).digest("hex").slice(0, 16);
}

function buildDedupeKey(
  source: CrawlSource,
  externalId: string | undefined,
  fallbackParts: Array<string | undefined>,
): string {
  if (externalId) {
    return `${source}:${externalId}`;
  }

  const seed = fallbackParts
    .map((item) => item?.trim().toLowerCase() ?? "")
    .join("|");
  return `${source}:sha1:${buildHash(seed)}`;
}

export function normalizeJobRows(source: CrawlSource, rows: unknown[]): NormalizedJobRow[] {
  const normalized: NormalizedJobRow[] = [];
  const seenKeys = new Set<string>();

  for (const item of rows) {
    const row = asRecord(item);
    if (!row) {
      continue;
    }

    const title = asString(row.title);
    const company = asString(row.company);
    if (!title || !company) {
      continue;
    }

    const city = asString(row.city);
    const salaryK = asString(row.salaryK);
    const jobUrl = asString(row.jobUrl);
    const description = asString(row.description);
    const externalId =
      asString(row.externalId) ??
      `${source}-job-${buildHash([title, company, city, salaryK, jobUrl].join("|"))}`;
    const dedupeKey = buildDedupeKey(source, externalId, [title, company, city, salaryK, jobUrl]);
    if (seenKeys.has(dedupeKey)) {
      continue;
    }

    seenKeys.add(dedupeKey);
    normalized.push({
      externalId,
      source,
      title,
      company,
      city,
      salaryK,
      jobUrl,
      description,
      dedupeKey,
    });
  }

  return normalized;
}

export function normalizeCandidateRows(source: CrawlSource, rows: unknown[]): NormalizedCandidateRow[] {
  const normalized: NormalizedCandidateRow[] = [];
  const seenKeys = new Set<string>();

  for (const item of rows) {
    const row = asRecord(item);
    if (!row) {
      continue;
    }

    const name = asString(row.name);
    if (!name) {
      continue;
    }

    const years = asNumber(row.years) ?? asNumber(row.yearsOfExperience) ?? 0;
    const currentCompany = asString(row.currentCompany);
    const tag = asString(row.tag);
    const externalId =
      asString(row.externalId) ??
      `${source}-candidate-${buildHash([name, currentCompany, String(years)].join("|"))}`;
    const dedupeKey = buildDedupeKey(source, externalId, [name, currentCompany, String(years)]);
    if (seenKeys.has(dedupeKey)) {
      continue;
    }

    seenKeys.add(dedupeKey);
    normalized.push({
      externalId,
      source,
      name,
      currentCompany,
      years,
      tag,
      phone: asString(row.phone),
      email: asString(row.email),
      dedupeKey,
    });
  }

  return normalized;
}

export function normalizeResumePayload(payload: unknown): NormalizedResumePayload | null {
  const record = asRecord(payload);
  if (!record) {
    return null;
  }

  const rawText = asString(record.rawText);
  const parsed = asRecord(record.parsed);
  if (!rawText || !parsed) {
    return null;
  }

  return {
    rawText,
    parsed,
  };
}
