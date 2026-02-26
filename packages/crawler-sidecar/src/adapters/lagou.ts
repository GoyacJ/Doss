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

export type LagouJobRow = NormalizedJobRow;

interface LagouAdapterOptions {
  sessionDir: string;
  headless: boolean;
}

export function buildLagouSearchUrl(params: CrawlJobsParams): string {
  const url = new URL("https://www.lagou.com/wn/jobs");
  url.searchParams.set("kd", params.keyword.trim());
  if (params.city?.trim()) {
    url.searchParams.set("city", params.city.trim());
  }
  if (params.page && params.page > 1) {
    url.searchParams.set("pn", String(params.page));
  }
  return url.toString();
}

export function buildLagouCandidatesUrl(params: CrawlCandidatesParams): string {
  const url = new URL("https://easy.lagou.com/can/list.htm");
  url.searchParams.set("positionId", params.jobId.trim());
  if (params.page && params.page > 1) {
    url.searchParams.set("pageNo", String(params.page));
  }
  return url.toString();
}

export function buildLagouResumeUrl(candidateId: string): string {
  const url = new URL("https://easy.lagou.com/can/resume/detail.htm");
  url.searchParams.set("resumeId", candidateId.trim());
  return url.toString();
}

export function normalizeLagouJobRows(rows: unknown[]): LagouJobRow[] {
  return normalizeJobRows("lagou", rows);
}

export class LagouAdapter implements SourceAdapter {
  public readonly source = "lagou" as const;

  private readonly jobSelectors: PageFieldSelectors = {
    cards: [
      ".item.__open",
      ".job-card-wrapper",
      ".position-card",
      ".job-item",
      ".list-item",
    ],
    title: [".p-top__1", ".position-name", ".job-name", "h3", "h4"],
    company: [".company-name", ".company__name", ".company", ".p-bom__company", ".company-item__title"],
    city: [".p-bom__city", ".city", ".job-area", ".position-label"],
    salary: [".p-top__2", ".salary", ".money", ".job-salary"],
    description: [".ir___3", ".position__labels", ".tags", ".job-detail"],
    link: ["a[href*='lagou.com']", "a[href*='jobs']"],
  };

  private readonly candidateSelectors: PageFieldSelectors = {
    cards: [
      ".candidate-item",
      ".resume-item",
      ".recommend-item",
      ".list-item",
      ".card-item",
    ],
    name: [".name", ".candidate-name", ".resume-name", "h3", "h4"],
    company: [".company", ".current-company", ".work-company", ".company-name"],
    years: [".work-year", ".experience", ".years", ".resume-info"],
    tag: [".tag", ".status", ".label"],
    phone: [".phone", ".mobile", ".tel"],
    email: [".email"],
    link: ["a[href*='resume']", "a[href*='lagou.com']", "a"],
  };

  private readonly resumeSelectors: ResumeSelectors = {
    containers: [
      ".resume-content",
      ".resume-detail",
      ".resume-body",
      ".resume-main",
      ".content-main",
      "main",
    ],
  };

  constructor(private readonly options: LagouAdapterOptions) {}

  async checkSession(): Promise<{ valid: boolean; message?: string }> {
    try {
      return await withPersistentContext(this.options, async (context, page) => {
        await navigateAndStabilize(
          page,
          buildLagouSearchUrl({
            keyword: "前端",
          }),
          "compliant",
        );
        const cookies = await context.cookies();
        const valid = cookies.some((cookie) => cookie.domain.includes("lagou.com"));

        return {
          valid,
          message: valid
            ? "Lagou session detected from persistent browser profile"
            : "Lagou session cookie not found, please login in persistent profile",
        };
      });
    } catch (error) {
      return {
        valid: false,
        message: error instanceof Error ? error.message : "Failed to check Lagou session",
      };
    }
  }

  async crawlJobs(mode: CrawlMode, params: CrawlJobsParams): Promise<unknown[]> {
    return withPersistentContext(this.options, async (_context, page) => {
      const targetUrl = buildLagouSearchUrl(params);
      await navigateAndStabilize(page, targetUrl, mode);
      await assertPageAvailable(page, this.source);

      const rawRows = await extractJobCards(page, this.jobSelectors);
      const normalized = normalizeLagouJobRows(rawRows);
      if (normalized.length === 0) {
        throw new Error("lagou_jobs_parse_empty");
      }
      return normalized;
    });
  }

  async crawlCandidates(mode: CrawlMode, params: CrawlCandidatesParams): Promise<unknown[]> {
    return withPersistentContext(this.options, async (_context, page) => {
      const targetUrl = buildLagouCandidatesUrl(params);
      await navigateAndStabilize(page, targetUrl, mode);
      await assertPageAvailable(page, this.source);

      const rawRows = await extractCandidateCards(page, this.candidateSelectors);
      const normalized = normalizeCandidateRows(this.source, rawRows);
      if (normalized.length === 0) {
        throw new Error("lagou_candidates_parse_empty");
      }
      return normalized;
    });
  }

  async crawlResume(
    mode: CrawlMode,
    candidateId: string,
  ): Promise<{ rawText: string; parsed: Record<string, unknown> }> {
    return withPersistentContext(this.options, async (_context, page) => {
      const targetUrl = resolveDetailUrl(candidateId, buildLagouResumeUrl);
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
        throw new Error("lagou_resume_normalize_failed");
      }
      return normalized;
    });
  }
}
