import { describe, expect, it } from "vitest";
import type { ScreeningTemplateRecord } from "../services/backend";
import { resolveOverrideTemplateOptions } from "./screening-template-options";

function buildTemplate(
  id: number,
  name: string,
  updated_at: string,
): ScreeningTemplateRecord {
  return {
    id,
    scope: "global",
    job_id: null,
    name,
    dimensions: [
      {
        key: "goal_orientation",
        label: "目标导向",
        weight: 100,
      },
    ],
    risk_rules: {},
    created_at: updated_at,
    updated_at,
  };
}

describe("resolveOverrideTemplateOptions", () => {
  it("returns empty list when only default template exists", () => {
    const templates = [
      buildTemplate(1, "默认筛选模板", "2026-02-26T10:00:00Z"),
    ];

    expect(resolveOverrideTemplateOptions(templates)).toEqual([]);
  });

  it("excludes resident default template and keeps custom templates", () => {
    const templates = [
      buildTemplate(1, "默认筛选模板", "2026-02-26T09:00:00Z"),
      buildTemplate(2, "模板B", "2026-02-26T11:00:00Z"),
      buildTemplate(3, "模板C", "2026-02-26T10:00:00Z"),
    ];

    const result = resolveOverrideTemplateOptions(templates);
    expect(result.map((item) => item.id)).toEqual([2, 3]);
  });

  it("falls back to first template as default when default name is missing", () => {
    const templates = [
      buildTemplate(11, "主模板", "2026-02-26T09:00:00Z"),
      buildTemplate(12, "模板B", "2026-02-26T11:00:00Z"),
      buildTemplate(13, "模板C", "2026-02-26T10:00:00Z"),
    ];

    const result = resolveOverrideTemplateOptions(templates);
    expect(result.map((item) => item.id)).toEqual([12, 13]);
  });
});
