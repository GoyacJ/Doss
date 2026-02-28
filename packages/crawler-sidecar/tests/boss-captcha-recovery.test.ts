import { beforeEach, describe, expect, it, vi } from "vitest";

const base = vi.hoisted(() => ({
  withPersistentContext: vi.fn(),
  navigateAndStabilize: vi.fn(),
  assertPageAvailable: vi.fn(),
  extractJobCards: vi.fn(),
  extractCandidateCards: vi.fn(),
  extractResumePayload: vi.fn(),
  resolveDetailUrl: vi.fn((candidateId: string, builder: (id: string) => string) => builder(candidateId)),
}));

vi.mock("../src/adapters/base", () => base);

import { BossAdapter } from "../src/adapters/boss";

describe("BossAdapter captcha recovery", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("recovers from captcha by opening headful context and retrying jobs crawl", async () => {
    const page = {
      waitForTimeout: vi.fn().mockResolvedValue(undefined),
      url: vi.fn().mockReturnValue("https://www.zhipin.com/web/geek/job?query=前端"),
      title: vi.fn().mockResolvedValue("职位搜索 - BOSS直聘"),
    };

    const optionsHistory: Array<{ headless: boolean; sessionDir: string }> = [];
    base.withPersistentContext.mockImplementation(async (options, callback) => {
      optionsHistory.push(options);
      return callback({ cookies: async () => [] }, page);
    });

    let assertCall = 0;
    base.assertPageAvailable.mockImplementation(async () => {
      assertCall += 1;
      if (assertCall === 1 || assertCall === 2) {
        throw new Error("boss_captcha_or_blocked");
      }
    });

    base.extractJobCards.mockResolvedValue([
      {
        title: "前端工程师",
        company: "示例科技",
      },
    ]);

    const adapter = new BossAdapter({
      sessionDir: "/tmp/doss-boss-session",
      headless: true,
    });

    const rows = await adapter.crawlJobs("compliant", {
      keyword: "前端",
    });

    expect(rows).toHaveLength(1);
    expect(base.extractJobCards).toHaveBeenCalledTimes(1);
    expect(optionsHistory.map((item) => item.headless)).toEqual([true, false, false]);
  });

  it("recovers from session invalid by opening headful context and retrying jobs crawl", async () => {
    const page = {
      waitForTimeout: vi.fn().mockResolvedValue(undefined),
      url: vi.fn().mockReturnValue("https://www.zhipin.com/web/geek/job?query=前端"),
      title: vi.fn().mockResolvedValue("职位搜索 - BOSS直聘"),
    };

    const optionsHistory: Array<{ headless: boolean; sessionDir: string }> = [];
    base.withPersistentContext.mockImplementation(async (options, callback) => {
      optionsHistory.push(options);
      return callback({ cookies: async () => [] }, page);
    });

    let assertCall = 0;
    base.assertPageAvailable.mockImplementation(async () => {
      assertCall += 1;
      if (assertCall === 1 || assertCall === 2) {
        throw new Error("boss_session_invalid_or_login_required");
      }
    });

    base.extractJobCards.mockResolvedValue([
      {
        title: "前端工程师",
        company: "示例科技",
      },
    ]);

    const adapter = new BossAdapter({
      sessionDir: "/tmp/doss-boss-session",
      headless: true,
    });

    const rows = await adapter.crawlJobs("compliant", {
      keyword: "前端",
    });

    expect(rows).toHaveLength(1);
    expect(base.extractJobCards).toHaveBeenCalledTimes(1);
    expect(optionsHistory.map((item) => item.headless)).toEqual([true, false, false]);
  });
});
