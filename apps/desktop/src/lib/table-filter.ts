function toNormalizedText(value: unknown): string {
  if (value === null || value === undefined) {
    return "";
  }
  if (typeof value === "boolean") {
    return value ? "true 是 yes" : "false 否 no";
  }
  if (typeof value === "number") {
    return Number.isFinite(value) ? String(value) : "";
  }
  if (value instanceof Date) {
    return value.toISOString();
  }
  if (Array.isArray(value)) {
    return value.map((item) => toNormalizedText(item)).join(" ");
  }
  if (typeof value === "object") {
    return Object.values(value as Record<string, unknown>)
      .map((item) => toNormalizedText(item))
      .join(" ");
  }
  return String(value);
}

export function normalizeKeyword(keyword: string): string {
  return keyword.trim().toLowerCase();
}

export function includesKeyword(keyword: string, ...values: unknown[]): boolean {
  const normalized = normalizeKeyword(keyword);
  if (!normalized) {
    return true;
  }

  return values
    .map((value) => toNormalizedText(value).toLowerCase())
    .some((text) => text.includes(normalized));
}
