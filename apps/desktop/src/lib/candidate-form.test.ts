import { describe, expect, it } from "vitest";
import { buildCandidateManualPayload, normalizeCandidateTags } from "./candidate-form";

describe("candidate form helpers", () => {
  it("normalizes and deduplicates tags from multiple separators", () => {
    expect(normalizeCandidateTags(" Vue,TypeScript，远程\nvue  ")).toEqual([
      "Vue",
      "TypeScript",
      "远程",
    ]);
  });

  it("builds payload with trimmed values and optional fields", () => {
    const result = buildCandidateManualPayload({
      name: " 张三 ",
      currentCompany: " 示例科技 ",
      jobId: 18,
      score: 85,
      age: 28,
      gender: "male",
      yearsOfExperience: 6,
      address: " 上海 ",
      phone: " 13800000000 ",
      email: " zhangsan@example.com ",
      tagsText: "Vue, TypeScript",
    });

    expect(result).toEqual({
      ok: true,
      payload: {
        name: "张三",
        current_company: "示例科技",
        job_id: 18,
        score: 85,
        age: 28,
        gender: "male",
        years_of_experience: 6,
        address: "上海",
        phone: "13800000000",
        email: "zhangsan@example.com",
        tags: ["Vue", "TypeScript"],
      },
    });
  });

  it("returns validation error when required fields are invalid", () => {
    expect(buildCandidateManualPayload({
      name: "  ",
      currentCompany: "",
      jobId: 0,
      score: null,
      age: null,
      gender: "",
      yearsOfExperience: 3,
      address: "",
      phone: "",
      email: "",
      tagsText: "",
    })).toEqual({
      ok: false,
      error: "请填写候选人姓名",
    });

    expect(buildCandidateManualPayload({
      name: "李四",
      currentCompany: "",
      jobId: 0,
      score: null,
      age: null,
      gender: "",
      yearsOfExperience: -1,
      address: "",
      phone: "",
      email: "",
      tagsText: "",
    })).toEqual({
      ok: false,
      error: "请填写有效工作年限",
    });
  });

  it("returns validation error when score is outside 0-100", () => {
    expect(buildCandidateManualPayload({
      name: "王五",
      currentCompany: "",
      jobId: 0,
      score: 101,
      age: null,
      gender: "",
      yearsOfExperience: 2,
      address: "",
      phone: "",
      email: "",
      tagsText: "",
    })).toEqual({
      ok: false,
      error: "评分范围需在 0-100 之间",
    });
  });
});
