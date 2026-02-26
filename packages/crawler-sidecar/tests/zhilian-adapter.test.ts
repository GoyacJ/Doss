import { describe, expect, it } from "vitest";
import {
  buildZhilianCandidatesUrl,
  buildZhilianResumeUrl,
  buildZhilianSearchUrl,
  normalizeZhilianJobRows,
} from "../src/adapters/zhilian";

describe("buildZhilianSearchUrl", () => {
  it("builds encoded Zhilian search url", () => {
    const url = buildZhilianSearchUrl({
      keyword: "前端 开发",
      city: "上海",
      page: 2,
    });
    const parsed = new URL(url);

    expect(url).toContain("sou.zhaopin.com");
    expect(parsed.searchParams.get("kw")).toBe("前端 开发");
    expect(parsed.searchParams.get("jl")).toBe("上海");
    expect(parsed.searchParams.get("p")).toBe("2");
  });
});

describe("normalizeZhilianJobRows", () => {
  it("normalizes valid rows and drops malformed rows", () => {
    const rows = normalizeZhilianJobRows([
      {
        title: "前端工程师",
        company: "示例科技",
        city: "上海",
        salaryK: "30-45",
      },
      {
        title: "",
        company: "invalid",
      },
    ]);

    expect(rows).toHaveLength(1);
    expect(rows[0]?.source).toBe("zhilian");
    expect(rows[0]?.title).toBe("前端工程师");
  });
});

describe("build zhilian candidates/resume urls", () => {
  it("builds candidates url", () => {
    const url = buildZhilianCandidatesUrl({
      jobId: "JN123",
      page: 3,
    });
    const parsed = new URL(url);

    expect(parsed.searchParams.get("positionNumber")).toBe("JN123");
    expect(parsed.searchParams.get("page")).toBe("3");
  });

  it("builds resume url", () => {
    const url = buildZhilianResumeUrl("RN456");
    const parsed = new URL(url);

    expect(parsed.searchParams.get("resumeNumber")).toBe("RN456");
  });
});
