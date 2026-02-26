import { describe, expect, it } from "vitest";
import {
  buildWubaCandidatesUrl,
  buildWubaResumeUrl,
  buildWubaSearchUrl,
  normalizeWubaJobRows,
} from "../src/adapters/wuba";

describe("buildWubaSearchUrl", () => {
  it("builds encoded 58 search url", () => {
    const url = buildWubaSearchUrl({
      keyword: "招聘 专员",
      city: "广州",
      page: 2,
    });
    const parsed = new URL(url);

    expect(url).toContain("58.com/job.shtml");
    expect(parsed.searchParams.get("key")).toBe("招聘 专员");
    expect(parsed.searchParams.get("city")).toBe("广州");
    expect(parsed.searchParams.get("page")).toBe("2");
  });
});

describe("normalizeWubaJobRows", () => {
  it("normalizes valid rows and drops malformed rows", () => {
    const rows = normalizeWubaJobRows([
      {
        title: "招聘专员",
        company: "城市服务集团",
        city: "广州",
        salaryK: "12-18",
      },
      {
        company: "invalid",
      },
    ]);

    expect(rows).toHaveLength(1);
    expect(rows[0]?.source).toBe("wuba");
    expect(rows[0]?.title).toBe("招聘专员");
  });
});

describe("build wuba candidates/resume urls", () => {
  it("builds candidates url", () => {
    const url = buildWubaCandidatesUrl({
      jobId: "58-JOB-1",
      page: 4,
    });
    const parsed = new URL(url);

    expect(parsed.searchParams.get("jobId")).toBe("58-JOB-1");
    expect(parsed.searchParams.get("page")).toBe("4");
  });

  it("builds resume url", () => {
    const url = buildWubaResumeUrl("58-RS-88");
    const parsed = new URL(url);

    expect(parsed.searchParams.get("resumeId")).toBe("58-RS-88");
  });
});
