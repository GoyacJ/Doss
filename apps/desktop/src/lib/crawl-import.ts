export interface JobImportItem {
  external_id?: string;
  title: string;
  company: string;
  city?: string;
  salary_k?: string;
  description?: string;
}

export interface CandidateImportItem {
  external_id?: string;
  name: string;
  current_company?: string;
  age?: number;
  address?: string;
  years_of_experience: number;
  tags: string[];
  phone?: string;
  email?: string;
}

export interface ResumeImportItem {
  raw_text: string;
  parsed: Record<string, unknown>;
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return null;
  }

  return value as Record<string, unknown>;
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

export function extractJobImportItems(sidecarResult: unknown): JobImportItem[] {
  const record = asRecord(sidecarResult);
  if (!record) {
    return [];
  }

  const status = asString(record.status);
  if (status !== "SUCCEEDED") {
    return [];
  }

  const output = record.output;
  if (!Array.isArray(output)) {
    return [];
  }

  const rows: JobImportItem[] = [];
  for (const item of output) {
    const row = asRecord(item);
    if (!row) {
      continue;
    }

    const title = asString(row.title);
    const company = asString(row.company);
    if (!title || !company) {
      continue;
    }

    rows.push({
      external_id: asString(row.externalId),
      title,
      company,
      city: asString(row.city),
      salary_k: asString(row.salaryK),
      description: asString(row.description),
    });
  }

  return rows;
}

export function extractCandidateImportItems(
  sidecarResult: unknown,
): CandidateImportItem[] {
  const record = asRecord(sidecarResult);
  if (!record) {
    return [];
  }

  const status = asString(record.status);
  if (status !== "SUCCEEDED") {
    return [];
  }

  const output = record.output;
  if (!Array.isArray(output)) {
    return [];
  }

  const rows: CandidateImportItem[] = [];
  for (const item of output) {
    const row = asRecord(item);
    if (!row) {
      continue;
    }

    const name = asString(row.name);
    const years = asNumber(row.years);
    if (!name || typeof years !== "number") {
      continue;
    }

    const tag = asString(row.tag);
    const age = asNumber(row.age);
    const normalizedAge = typeof age === "number" && age >= 0 ? Math.trunc(age) : undefined;
    rows.push({
      external_id: asString(row.externalId),
      name,
      current_company: asString(row.currentCompany),
      age: normalizedAge,
      address: asString(row.address) ?? asString(row.location),
      years_of_experience: years,
      tags: tag ? [tag] : [],
      phone: asString(row.phone),
      email: asString(row.email),
    });
  }

  return rows;
}

export function extractResumeImportItem(
  sidecarResult: unknown,
): ResumeImportItem | null {
  const record = asRecord(sidecarResult);
  if (!record) {
    return null;
  }

  const status = asString(record.status);
  if (status !== "SUCCEEDED") {
    return null;
  }

  const output = asRecord(record.output);
  if (!output) {
    return null;
  }

  const rawText = asString(output.rawText);
  const parsed = asRecord(output.parsed);
  if (!rawText || !parsed) {
    return null;
  }

  return {
    raw_text: rawText,
    parsed,
  };
}
