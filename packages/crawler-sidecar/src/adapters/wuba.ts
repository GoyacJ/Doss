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

export type WubaJobRow = NormalizedJobRow;

interface WubaAdapterOptions {
  sessionDir: string;
  headless: boolean;
}

export function buildWubaSearchUrl(params: CrawlJobsParams): string {
  const url = new URL("https://www.58.com/job.shtml");
  url.searchParams.set("key", params.keyword.trim());
  if (params.city?.trim()) {
    url.searchParams.set("city", params.city.trim());
  }
  if (params.page && params.page > 1) {
    url.searchParams.set("page", String(params.page));
  }
  return url.toString();
}

export function buildWubaCandidatesUrl(params: CrawlCandidatesParams): string {
  const url = new URL("https://vip.58.com/zhaopin/candidates");
  url.searchParams.set("jobId", params.jobId.trim());
  if (params.page && params.page > 1) {
    url.searchParams.set("page", String(params.page));
  }
  return url.toString();
}

export function buildWubaResumeUrl(candidateId: string): string {
  const url = new URL("https://vip.58.com/zhaopin/resume/detail");
  url.searchParams.set("resumeId", candidateId.trim());
  return url.toString();
}

export function normalizeWubaJobRows(rows: unknown[]): WubaJobRow[] {
  return normalizeJobRows("wuba", rows);
}

export class WubaAdapter implements SourceAdapter {
  public readonly source = "wuba" as const;

  private readonly jobSelectors: PageFieldSelectors = {
    cards: [
      ".job_item",
      ".job-item",
      ".list-item",
      ".job-list-item",
      ".job-item-wrapper",
    ],
    title: [".item_title", ".job-name", ".title", "h3", "h4"],
    company: [".comp_name", ".company-name", ".company", ".cname"],
    city: [".job-area", ".local", ".workplace", ".city"],
    salary: [".job_salary", ".salary", ".pay", ".wage"],
    description: [".item_desc", ".desc", ".tags", ".welfare"],
    link: ["a[href*='58.com']", "a[href*='job']"],
  };

  private readonly candidateSelectors: PageFieldSelectors = {
    cards: [
      ".candidate-item",
      ".resume-item",
      ".recommend-item",
      ".list-item",
      ".resume-list-item",
    ],
    name: [".name", ".candidate-name", ".resume-name", "h3", "h4"],
    company: [".company", ".current-company", ".work-company", ".company-name"],
    years: [".work-year", ".experience", ".years", ".resume-info"],
    tag: [".tag", ".status", ".label"],
    phone: [".phone", ".mobile", ".tel"],
    email: [".email"],
    link: ["a[href*='resume']", "a[href*='58.com']", "a"],
  };

  private readonly resumeSelectors: ResumeSelectors = {
    containers: [
      ".resume-content",
      ".resume-detail",
      ".resume-body",
      ".resume-main",
      ".summary",
      "main",
    ],
  };

  constructor(private readonly options: WubaAdapterOptions) {}

  async checkSession(): Promise<{ valid: boolean; message?: string }> {
    try {
      return await withPersistentContext(this.options, async (context, page) => {
        await navigateAndStabilize(
          page,
          buildWubaSearchUrl({
            keyword: "前端",
          }),
          "compliant",
        );
        const cookies = await context.cookies();
        const valid = cookies.some((cookie) => cookie.domain.includes("58.com"));

        return {
          valid,
          message: valid
            ? "58 session detected from persistent browser profile"
            : "58 session cookie not found, please login in persistent profile",
        };
      });
    } catch (error) {
      return {
        valid: false,
        message: error instanceof Error ? error.message : "Failed to check 58 session",
      };
    }
  }

  async crawlJobs(mode: CrawlMode, params: CrawlJobsParams): Promise<unknown[]> {
    return withPersistentContext(this.options, async (_context, page) => {
      const targetUrl = buildWubaSearchUrl(params);
      await navigateAndStabilize(page, targetUrl, mode);
      await assertPageAvailable(page, this.source);

      const rawRows = await extractJobCards(page, this.jobSelectors);
      const normalized = normalizeWubaJobRows(rawRows);
      if (normalized.length === 0) {
        throw new Error("wuba_jobs_parse_empty");
      }
      return normalized;
    });
  }

  async crawlCandidates(mode: CrawlMode, params: CrawlCandidatesParams): Promise<unknown[]> {
    return withPersistentContext(this.options, async (_context, page) => {
      const targetUrl = buildWubaCandidatesUrl(params);
      await navigateAndStabilize(page, targetUrl, mode);
      await assertPageAvailable(page, this.source);

      const rawRows = await extractCandidateCards(page, this.candidateSelectors);
      const normalized = normalizeCandidateRows(this.source, rawRows);
      if (normalized.length === 0) {
        throw new Error("wuba_candidates_parse_empty");
      }
      return normalized;
    });
  }

  async crawlResume(
    mode: CrawlMode,
    candidateId: string,
  ): Promise<{ rawText: string; parsed: Record<string, unknown> }> {
    return withPersistentContext(this.options, async (_context, page) => {
      const targetUrl = resolveDetailUrl(candidateId, buildWubaResumeUrl);
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
        throw new Error("wuba_resume_normalize_failed");
      }
      return normalized;
    });
  }
}
