import { describe, expect, it } from "vitest";
import {
  buildLagouCandidatesUrl,
  buildLagouResumeUrl,
  buildLagouSearchUrl,
  normalizeLagouJobRows,
} from "../src/adapters/lagou";

describe("buildLagouSearchUrl", () => {
  it("builds encoded Lagou search url", () => {
    const url = buildLagouSearchUrl({
      keyword: "前端 开发",
      city: "上海",
      page: 2,
    });
    const parsed = new URL(url);

    expect(url).toContain("lagou.com");
    expect(parsed.searchParams.get("kd")).toBe("前端 开发");
    expect(parsed.searchParams.get("city")).toBe("上海");
    expect(parsed.searchParams.get("pn")).toBe("2");
  });
});

describe("normalizeLagouJobRows", () => {
  it("normalizes valid rows and drops malformed rows", () => {
    const rows = normalizeLagouJobRows([
      {
        title: "高级前端工程师",
        company: "拉勾示例科技",
        city: "上海",
        salaryK: "35-50",
      },
      {
        title: "",
        company: "invalid",
      },
    ]);

    expect(rows).toHaveLength(1);
    expect(rows[0]?.source).toBe("lagou");
    expect(rows[0]?.title).toBe("高级前端工程师");
  });
});

describe("build lagou candidates/resume urls", () => {
  it("builds candidates url", () => {
    const url = buildLagouCandidatesUrl({
      jobId: "LG-JOB-1",
      page: 3,
    });
    const parsed = new URL(url);

    expect(parsed.searchParams.get("positionId")).toBe("LG-JOB-1");
    expect(parsed.searchParams.get("pageNo")).toBe("3");
  });

  it("builds resume url", () => {
    const url = buildLagouResumeUrl("LG-RS-88");
    const parsed = new URL(url);

    expect(parsed.searchParams.get("resumeId")).toBe("LG-RS-88");
  });
});
