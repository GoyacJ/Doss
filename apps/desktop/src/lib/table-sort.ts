import type { SortDirection, SortRule } from "@doss/shared";

export type SortPrimitive = string | number | boolean | Date | null | undefined;

export type SortResolver<Row, Field extends string> = Record<
  Field,
  (row: Row) => SortPrimitive
>;

function normalizeDirection(direction: SortDirection | string): SortDirection {
  return String(direction).toLowerCase() === "asc" ? "asc" : "desc";
}

function isNil(value: SortPrimitive): value is null | undefined {
  return value === null || value === undefined;
}

function compareNonNil(a: Exclude<SortPrimitive, null | undefined>, b: Exclude<SortPrimitive, null | undefined>): number {
  if (typeof a === "number" && typeof b === "number") {
    return a - b;
  }

  if (typeof a === "boolean" && typeof b === "boolean") {
    return Number(a) - Number(b);
  }

  if (a instanceof Date && b instanceof Date) {
    return a.getTime() - b.getTime();
  }

  const left = String(a).trim();
  const right = String(b).trim();
  return left.localeCompare(right, "zh-Hans-CN", { numeric: true, sensitivity: "base" });
}

function compareSortPrimitive(
  left: SortPrimitive,
  right: SortPrimitive,
  direction: SortDirection,
): number {
  if (isNil(left) && isNil(right)) {
    return 0;
  }
  if (isNil(left)) {
    return 1;
  }
  if (isNil(right)) {
    return -1;
  }

  const base = compareNonNil(left, right);
  return direction === "asc" ? base : -base;
}

export function normalizeSortRules<Field extends string>(
  rules: SortRule<Field>[] | undefined,
  allowedFields: readonly Field[],
  maxRules = 3,
): SortRule<Field>[] {
  if (!rules || rules.length === 0) {
    return [];
  }

  const allowed = new Set<string>(allowedFields);
  const seen = new Set<string>();
  const normalized: SortRule<Field>[] = [];

  for (const rawRule of rules) {
    const field = String(rawRule.field).trim() as Field;
    if (!field || !allowed.has(field) || seen.has(field)) {
      continue;
    }
    seen.add(field);
    normalized.push({
      field,
      direction: normalizeDirection(rawRule.direction),
    });
    if (normalized.length >= maxRules) {
      break;
    }
  }

  return normalized;
}

export function sortRowsByRules<Row, Field extends string>(
  rows: Row[],
  rules: SortRule<Field>[],
  resolver: SortResolver<Row, Field>,
): Row[] {
  if (rules.length === 0) {
    return rows.slice();
  }

  const stableRows = rows.map((item, index) => ({ item, index }));

  stableRows.sort((left, right) => {
    for (const rule of rules) {
      const getValue = resolver[rule.field];
      if (!getValue) {
        continue;
      }
      const compared = compareSortPrimitive(
        getValue(left.item),
        getValue(right.item),
        normalizeDirection(rule.direction),
      );
      if (compared !== 0) {
        return compared;
      }
    }
    return left.index - right.index;
  });

  return stableRows.map((entry) => entry.item);
}

export function sortSummary<Field extends string>(
  rules: SortRule<Field>[],
  labels: Record<Field, string>,
): string {
  if (rules.length === 0) {
    return "默认排序";
  }

  return rules
    .map((rule, index) => {
      const label = labels[rule.field] ?? String(rule.field);
      const direction = rule.direction === "asc" ? "升序" : "降序";
      return `${index + 1}. ${label}${direction}`;
    })
    .join(" · ");
}
