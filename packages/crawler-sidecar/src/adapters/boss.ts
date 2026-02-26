import type { CrawlMode } from "@doss/shared";
import {
  assertPageAvailable,
  extractCandidateCards,
  extractJobCards,
  extractResumePayload,
  navigateAndStabilize,
  resolveDetailUrl,
  type PageFieldSelectors,
  type ResumeSelectors,
  withPersistentContext,
} from "./base";
import {
  normalizeCandidateRows,
  normalizeJobRows,
  normalizeResumePayload,
  type NormalizedJobRow,
} from "./normalize";
import type { CrawlCandidatesParams, CrawlJobsParams, SourceAdapter } from "./types";

export type BossJobRow = NormalizedJobRow;

interface BossAdapterOptions {
  sessionDir: string;
  headless: boolean;
}

export function buildBossSearchUrl(params: CrawlJobsParams): string {
  const url = new URL("https://www.zhipin.com/web/geek/job");
  url.searchParams.set("query", params.keyword.trim());

  if (params.city?.trim()) {
    url.searchParams.set("city", params.city.trim());
  }

  if (params.page && params.page > 1) {
    url.searchParams.set("page", String(params.page));
  }

  return url.toString();
}

export function normalizeBossJobRows(rows: unknown[]): BossJobRow[] {
  return normalizeJobRows("boss", rows);
}

export function buildBossCandidatesUrl(params: CrawlCandidatesParams): string {
  const url = new URL("https://www.zhipin.com/web/boss/recommend");
  url.searchParams.set("jobId", params.jobId.trim());
  if (params.page && params.page > 1) {
    url.searchParams.set("page", String(params.page));
  }
  return url.toString();
}

export function buildBossResumeUrl(candidateId: string): string {
  const url = new URL("https://www.zhipin.com/web/geek/chat");
  url.searchParams.set("uid", candidateId.trim());
  return url.toString();
}

export class BossAdapter implements SourceAdapter {
  public readonly source = "boss" as const;
  private readonly jobSelectors: PageFieldSelectors = {
    cards: [
      ".job-card-wrapper",
      ".job-list-box .job-card-wrapper",
      ".job-list-box .job-card",
      ".search-job-result .job-card-wrapper",
    ],
    title: [".job-name", ".job-title", "h3", "h4"],
    company: [".company-name", ".company-text", ".company-info a", ".company-name a"],
    city: [".job-area", ".job-area-wrapper", ".job-area-wrapper .city"],
    salary: [".salary", ".job-salary", ".red"],
    description: [".job-card-footer", ".info-desc", ".tags-box"],
    link: ["a.job-card-left", "a[href*='job_detail']", "a"],
  };
  private readonly candidateSelectors: PageFieldSelectors = {
    cards: [
      ".geek-item",
      ".candidate-item",
      ".recommend-geek-item",
      ".card-item",
      ".boss-geek-item",
    ],
    name: [".name", ".geek-name", ".candidate-name", ".user-name", "h3", "h4"],
    company: [".company", ".company-name", ".candidate-company", ".work-company"],
    years: [".year", ".work-year", ".experience", ".resume-age", ".info-text"],
    tag: [".tag", ".status", ".label", ".candidate-tag"],
    phone: [".phone", ".mobile", ".tel"],
    email: [".email"],
    link: ["a[href*='geek']", "a[href*='resume']", "a"],
  };
  private readonly resumeSelectors: ResumeSelectors = {
    containers: [
      ".resume-content",
      ".geek-resume-content",
      ".resume-box",
      ".chat-content",
      ".content-wrap",
      "main",
    ],
  };

  constructor(private readonly options: BossAdapterOptions) {
  }

  async checkSession(): Promise<{ valid: boolean; message?: string }> {
    try {
      return await withPersistentContext(
        this.options,
        async (context, page) => {
          await navigateAndStabilize(
            page,
            buildBossSearchUrl({
              keyword: "前端",
            }),
            "compliant",
          );
          const cookies = await context.cookies();
          const valid = cookies.some((cookie) => cookie.domain.includes("zhipin.com"));

          return {
            valid,
            message: valid
              ? "Boss session detected from persistent browser profile"
              : "Boss session cookie not found, please login in the persistent profile",
          };
        },
      );
    } catch (error) {
      return {
        valid: false,
        message: error instanceof Error ? error.message : "Failed to check Boss session",
      };
    }
  }

  async crawlJobs(mode: CrawlMode, params: CrawlJobsParams): Promise<unknown[]> {
    return withPersistentContext(this.options, async (_context, page) => {
      const targetUrl = buildBossSearchUrl(params);
      await navigateAndStabilize(page, targetUrl, mode);
      await assertPageAvailable(page, this.source);

      const rawRows = await extractJobCards(page, this.jobSelectors);
      const normalized = normalizeBossJobRows(rawRows);
      if (normalized.length === 0) {
        throw new Error("boss_jobs_parse_empty");
      }
      return normalized;
    });
  }

  async crawlCandidates(mode: CrawlMode, params: CrawlCandidatesParams): Promise<unknown[]> {
    return withPersistentContext(this.options, async (_context, page) => {
      const targetUrl = buildBossCandidatesUrl(params);
      await navigateAndStabilize(page, targetUrl, mode);
      await assertPageAvailable(page, this.source);

      const rawRows = await extractCandidateCards(page, this.candidateSelectors);
      const normalized = normalizeCandidateRows(this.source, rawRows);
      if (normalized.length === 0) {
        throw new Error("boss_candidates_parse_empty");
      }
      return normalized;
    });
  }

  async crawlResume(
    mode: CrawlMode,
    candidateId: string,
  ): Promise<{ rawText: string; parsed: Record<string, unknown> }> {
    return withPersistentContext(this.options, async (_context, page) => {
      const targetUrl = resolveDetailUrl(candidateId, buildBossResumeUrl);
      await navigateAndStabilize(page, targetUrl, mode);
      await assertPageAvailable(page, this.source);

      const payload = await extractResumePayload(
        page,
        this.source,
        candidateId,
        this.resumeSelectors,
      );
      const normalized = normalizeResumePayload(payload);
      if (!normalized) {
        throw new Error("boss_resume_normalize_failed");
      }
      return normalized;
    });
  }
}
