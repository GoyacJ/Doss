import type { ScoringTemplateRecord } from "../services/backend";

const RESIDENT_DEFAULT_TEMPLATE_NAME = "默认评分模板";

export function resolveResidentDefaultTemplate(
  templates: ScoringTemplateRecord[],
): ScoringTemplateRecord | null {
  if (templates.length === 0) {
    return null;
  }

  const namedDefault = templates.find((item) => item.name === RESIDENT_DEFAULT_TEMPLATE_NAME);
  if (namedDefault) {
    return namedDefault;
  }

  return templates[0] ?? null;
}

export function resolveOverrideTemplateOptions(
  templates: ScoringTemplateRecord[],
): ScoringTemplateRecord[] {
  const defaultTemplate = resolveResidentDefaultTemplate(templates);
  if (!defaultTemplate) {
    return [];
  }

  return templates.filter((item) => item.id !== defaultTemplate.id);
}
