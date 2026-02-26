import { describe, expect, it } from "vitest";
import {
  buildBossCandidatesUrl,
  buildBossResumeUrl,
  buildBossSearchUrl,
  normalizeBossJobRows,
} from "../src/adapters/boss";

describe("buildBossSearchUrl", () => {
  it("builds encoded Boss search url with keyword/city/page", () => {
    const url = buildBossSearchUrl({
      keyword: "前端 开发",
      city: "上海",
      page: 2,
    });
    const parsed = new URL(url);

    expect(url).toContain("zhipin.com/web/geek/job");
    expect(parsed.searchParams.get("query")).toBe("前端 开发");
    expect(parsed.searchParams.get("city")).toBe("上海");
    expect(parsed.searchParams.get("page")).toBe("2");
  });
});

describe("normalizeBossJobRows", () => {
  it("normalizes and filters malformed rows", () => {
    const rows = normalizeBossJobRows([
      {
        title: "前端工程师",
        company: "示例科技",
        city: "上海",
        salaryK: "30-45",
        jobUrl: "https://www.zhipin.com/job_detail/123.html",
      },
      {
        title: "",
        company: "缺失标题",
      },
    ]);

    expect(rows).toHaveLength(1);
    expect(rows[0]!).toMatchObject({
      title: "前端工程师",
      company: "示例科技",
      city: "上海",
      salaryK: "30-45",
      source: "boss",
    });
    expect(rows[0]!.externalId).toContain("boss-job-");
  });
});

describe("boss candidates/resume url builders", () => {
  it("builds candidates url from job id and page", () => {
    const url = buildBossCandidatesUrl({
      jobId: "job-123",
      page: 3,
    });
    const parsed = new URL(url);

    expect(url).toContain("zhipin.com/web/boss/recommend");
    expect(parsed.searchParams.get("jobId")).toBe("job-123");
    expect(parsed.searchParams.get("page")).toBe("3");
  });

  it("builds resume url from candidate id", () => {
    const url = buildBossResumeUrl("geek-99");
    const parsed = new URL(url);

    expect(url).toContain("zhipin.com/web/geek/chat");
    expect(parsed.searchParams.get("uid")).toBe("geek-99");
  });
});
