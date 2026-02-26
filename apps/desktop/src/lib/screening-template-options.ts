import type { ScreeningTemplateRecord } from "../services/backend";

export function resolveOverrideTemplateOptions(
  templates: ScreeningTemplateRecord[],
): ScreeningTemplateRecord[] {
  if (templates.length <= 1) {
    return [];
  }

  const latest = [...templates]
    .sort((left, right) => right.updated_at.localeCompare(left.updated_at))
    [0];
  if (!latest) {
    return [];
  }

  return templates.filter((item) => item.id !== latest.id);
}
