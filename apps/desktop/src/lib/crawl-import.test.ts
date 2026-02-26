import { describe, expect, it } from "vitest";
import {
  extractCandidateImportItems,
  extractJobImportItems,
  extractResumeImportItem,
} from "./crawl-import";

describe("extractJobImportItems", () => {
  it("extracts valid job rows from sidecar queue result", () => {
    const result = {
      status: "SUCCEEDED",
      output: [
        {
          externalId: "boss-job-1",
          title: "前端工程师",
          company: "示例科技",
          city: "上海",
          salaryK: "30-45",
          source: "boss",
        },
        {
          externalId: "boss-job-2",
          title: "高级前端工程师",
          company: "创新软件",
          city: "深圳",
          salaryK: "40-60",
          source: "boss",
        },
      ],
    };

    const jobs = extractJobImportItems(result);
    expect(jobs).toHaveLength(2);
    expect(jobs[0]).toEqual({
      external_id: "boss-job-1",
      title: "前端工程师",
      company: "示例科技",
      city: "上海",
      salary_k: "30-45",
      description: undefined,
    });
  });

  it("drops malformed rows and failed status payload", () => {
    const result = {
      status: "FAILED",
      output: [
        {
          title: "缺少公司字段",
        },
      ],
    };

    expect(extractJobImportItems(result)).toEqual([]);
  });
});

describe("extractCandidateImportItems", () => {
  it("extracts valid candidates from sidecar result", () => {
    const result = {
      status: "SUCCEEDED",
      output: [
        {
          externalId: "boss-candidate-1",
          name: "张三",
          currentCompany: "示例科技",
          years: 5,
          tag: "safe",
        },
        {
          externalId: "boss-candidate-2",
          name: "李四",
          currentCompany: "创新软件",
          years: 7,
          tag: "safe",
        },
      ],
    };

    const candidates = extractCandidateImportItems(result);
    expect(candidates).toHaveLength(2);
    expect(candidates[0]).toEqual({
      external_id: "boss-candidate-1",
      name: "张三",
      current_company: "示例科技",
      years_of_experience: 5,
      tags: ["safe"],
      phone: undefined,
      email: undefined,
    });
  });

  it("returns empty when status is not succeeded", () => {
    const result = {
      status: "FAILED",
      output: [
        {
          externalId: "boss-candidate-1",
          name: "张三",
        },
      ],
    };
    expect(extractCandidateImportItems(result)).toEqual([]);
  });
});

describe("extractResumeImportItem", () => {
  it("extracts resume payload from succeeded queue result", () => {
    const result = {
      status: "SUCCEEDED",
      output: {
        rawText: "candidate resume text",
        parsed: {
          skills: ["Vue3", "TypeScript"],
          expectedSalaryK: 45,
        },
      },
    };

    expect(extractResumeImportItem(result)).toEqual({
      raw_text: "candidate resume text",
      parsed: {
        skills: ["Vue3", "TypeScript"],
        expectedSalaryK: 45,
      },
    });
  });

  it("returns null when output malformed", () => {
    const result = {
      status: "SUCCEEDED",
      output: {
        parsed: {},
      },
    };
    expect(extractResumeImportItem(result)).toBeNull();
  });
});
