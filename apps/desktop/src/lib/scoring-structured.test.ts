import { describe, expect, it } from "vitest";
import type { ScoringResultRecord } from "../services/backend";
import { resolveStructuredScoringViewModel } from "./scoring-structured";

function buildScoringRecord(
  overrides: Partial<ScoringResultRecord> = {},
): ScoringResultRecord {
  return {
    id: 1,
    candidate_id: 10,
    job_id: 20,
    template_id: 3,
    overall_score: 82,
    overall_score_5: 4.1,
    t0_score_5: 4.3,
    t1_score_5: 4.0,
    t2_score_5: 3.8,
    t3_score_5: 3.5,
    recommendation: "PASS",
    risk_level: "MEDIUM",
    structured_result: {},
    created_at: "2026-03-01T00:00:00Z",
    ...overrides,
  };
}

describe("resolveStructuredScoringViewModel", () => {
  it("builds four modules from t0/t1/t2/t3 sections", () => {
    const model = resolveStructuredScoringViewModel(buildScoringRecord({
      structured_result: {
        summary: {
          overall_score_5: 4.2,
          overall_score_100: 84,
          overall_comment: "整体总结：候选人匹配度较高。",
          weights: { t0: 45, t1: 35, t2: 10, t3: 10 },
          subscores: { t0: 4.4, t1: 4.1, t2: 3.8, t3: 3.5 },
          recommendation: "PASS",
          risk_level: "MEDIUM",
        },
        template_assessment: {
          template: "后端评分模板",
          t0: {
            score_5: 4.4,
            comment: "T0 模块评价。",
            items: [{ key: "required_skills_match", label: "岗位技能匹配", description: "", weight: 50, score_5: 4.5, reason: "技能覆盖充分。", evidence: "简历技能列表" }],
          },
          t1: {
            score_5: 4.1,
            comment: "T1 模块评价。",
            items: [{ key: "goal_orientation", label: "目标导向", description: "", weight: 30, score_5: 4.2, reason: "目标意识明确。", evidence: "项目经历" }],
          },
          t2: {
            score_5: 3.8,
            comment: "T2 模块评价。",
            items: [{ key: "core_skill_bonus", label: "核心技能加分", description: "", weight: 40, score_5: 3.8, reason: "存在额外核心技能。", evidence: "技能清单" }],
          },
          t3: {
            score_5: 3.5,
            comment: "T3 模块评价。",
            items: [{ key: "salary_risk", label: "薪资风险", description: "", weight: 35, score_5: 3.5, reason: "预算差距可控。", evidence: "薪资信息" }],
          },
        },
      },
    }));

    expect(model).not.toBeNull();
    expect(model?.modules).toHaveLength(4);
    expect(model?.modules.map((item) => item.key)).toEqual(["t0", "t1", "t2", "t3"]);
    expect(model?.modules[0]?.title).toBe("T0 重要指标");
    expect(model?.modules[1]?.items[0]?.reason).toBe("目标意识明确。");
    expect(model?.overallComment).toBe("整体总结：候选人匹配度较高。");
  });

  it("keeps long overall comment as-is without truncation in view model", () => {
    const longComment = "长评".repeat(260);
    const model = resolveStructuredScoringViewModel(buildScoringRecord({
      structured_result: {
        summary: {
          overall_comment: longComment,
        },
      },
    }));

    expect(model).not.toBeNull();
    expect(model?.overallComment).toBe(longComment);
  });
});
