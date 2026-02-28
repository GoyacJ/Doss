import { describe, expect, it } from "vitest";
import { resolveStructuredScreeningViewModel } from "./screening-structured";

describe("resolveStructuredScreeningViewModel", () => {
  it("parses v2 structured result", () => {
    const model = resolveStructuredScreeningViewModel({
      id: 1,
      candidate_id: 8,
      job_id: 11,
      template_id: 3,
      t0_score: 4.2,
      t1_score: 80,
      fine_score: 76,
      bonus_score: 6,
      risk_penalty: 8,
      overall_score: 82,
      recommendation: "PASS",
      risk_level: "MEDIUM",
      evidence: [],
      verification_points: [],
      structured_result: {
        version: 2,
        summary: {
          overall_score_5: 4.1,
          overall_score_100: 82,
          weights: { t0: 50, t1: 30, t2: 10, t3: 10 },
          subscores: { t0: 4.2, t1: 4.0, t2: 2.0, t3: 3.0 },
          overall_comment: "建议进入下一轮面试。",
        },
        template_assessment: {
          template: "后端模板",
          items: [
            {
              key: "goal_orientation",
              label: "目标导向",
              weight: 30,
              score_5: 4,
              score_100: 80,
              comment: "目标结果清晰。",
            },
          ],
        },
        bonus_assessment: {
          score_5: 2.1,
          items: [{ title: "岗位技能全命中", score_5: 1.3, comment: "核心技能命中完整" }],
          comment: "具备明显加分项。",
        },
        risk_assessment: {
          level: "MEDIUM",
          score_5: 3,
          items: [{ title: "简历信息不足", severity: "MEDIUM", comment: "建议补充项目证据" }],
          comment: "存在中等风险，建议重点核验。",
        },
        risk_alerts: ["风险扣减 8"],
      },
      created_at: "2026-02-28T00:00:00Z",
    });

    expect(model).not.toBeNull();
    expect(model?.overallScore5).toBe(4.1);
    expect(model?.templateName).toBe("后端模板");
    expect(model?.templateItems).toHaveLength(1);
    expect(model?.bonusItems).toHaveLength(1);
    expect(model?.riskItems).toHaveLength(1);
    expect(model?.riskComment).toContain("中等风险");
  });

  it("falls back to v1 legacy shape", () => {
    const model = resolveStructuredScreeningViewModel({
      id: 2,
      candidate_id: 9,
      job_id: 12,
      template_id: 4,
      t0_score: 3.6,
      t1_score: 72,
      fine_score: 70,
      bonus_score: 4,
      risk_penalty: 10,
      overall_score: 74,
      recommendation: "REVIEW",
      risk_level: "HIGH",
      evidence: [],
      verification_points: [],
      structured_result: {
        weights: {
          t0: { score: 3.6, rule: "<3不匹配，3-4建议，>=4匹配", matched: true },
          t1: {
            template: "默认筛选模板",
            items: [{ key: "goal_orientation", label: "目标导向", weight: 30, score: 76, reason: "表现较好" }],
          },
          t2: { bonus: 4, items: ["技能有亮点"] },
        },
        risk_alerts: ["风险扣减 10", "薪资预期偏高"],
        overall_score: 74,
        overall_comment: "建议复核",
      },
      created_at: "2026-02-28T00:00:00Z",
    });

    expect(model).not.toBeNull();
    expect(model?.overallScore100).toBe(74);
    expect(model?.templateName).toBe("默认筛选模板");
    expect(model?.templateItems[0]?.score100).toBe(76);
    expect(model?.bonusItems[0]?.title).toBe("技能有亮点");
    expect(model?.riskItems[0]?.title).toBe("风险扣减 10");
  });

  it("returns null when screening missing", () => {
    expect(resolveStructuredScreeningViewModel(null)).toBeNull();
    expect(resolveStructuredScreeningViewModel(undefined)).toBeNull();
  });
});
