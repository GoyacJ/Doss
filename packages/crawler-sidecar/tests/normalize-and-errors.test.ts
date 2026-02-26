import { describe, expect, it } from "vitest";
import {
  normalizeCandidateRows,
  normalizeJobRows,
  normalizeResumePayload,
} from "../src/adapters/normalize";
import { classifyCrawlError } from "../src/errors";

describe("normalizeJobRows", () => {
  it("standardizes rows and adds dedupeKey", () => {
    const rows = normalizeJobRows("boss", [
      {
        externalId: "boss-job-123",
        title: "前端工程师",
        company: "示例科技",
        city: "上海",
        salaryK: "30-45",
        jobUrl: "https://www.zhipin.com/job_detail/123.html",
      },
      {
        title: "后端工程师",
        company: "示例科技",
        city: "上海",
      },
      {
        title: "",
        company: "invalid",
      },
    ]);

    expect(rows).toHaveLength(2);
    expect(rows[0]?.dedupeKey).toBe("boss:boss-job-123");
    expect(rows[1]?.dedupeKey).toContain("boss:");
    expect(rows[1]?.title).toBe("后端工程师");
  });
});

describe("normalizeCandidateRows", () => {
  it("standardizes candidates and computes dedupe key", () => {
    const rows = normalizeCandidateRows("boss", [
      {
        externalId: "boss-candidate-1",
        name: "张三",
        years: 5,
        currentCompany: "示例科技",
      },
      {
        name: "李四",
        years: 3,
      },
      {
        name: "",
      },
    ]);

    expect(rows).toHaveLength(2);
    expect(rows[0]?.dedupeKey).toBe("boss:boss-candidate-1");
    expect(rows[1]?.dedupeKey).toContain("boss:");
  });
});

describe("normalizeResumePayload", () => {
  it("returns normalized resume payload when valid", () => {
    const payload = normalizeResumePayload({
      rawText: "candidate resume",
      parsed: { skills: ["Vue3"] },
    });

    expect(payload).toEqual({
      rawText: "candidate resume",
      parsed: { skills: ["Vue3"] },
    });
  });

  it("returns null on malformed payload", () => {
    expect(normalizeResumePayload({ parsed: {} })).toBeNull();
  });
});

describe("classifyCrawlError", () => {
  it("classifies timeout and returns snapshot", () => {
    const detail = classifyCrawlError(new Error("Navigation timeout of 20000 ms exceeded"), {
      source: "boss",
      taskType: "jobs",
      mode: "compliant",
      payload: { keyword: "前端" },
      url: "https://www.zhipin.com/web/geek/job",
    });

    expect(detail.errorCode).toBe("TIMEOUT");
    expect(detail.snapshot.source).toBe("boss");
  });

  it("classifies captcha-like errors", () => {
    const detail = classifyCrawlError(new Error("需要验证码后继续"), {
      source: "boss",
      taskType: "jobs",
      mode: "compliant",
      payload: {},
    });

    expect(detail.errorCode).toBe("CAPTCHA_REQUIRED");
  });
});
