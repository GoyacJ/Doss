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

export type ZhilianJobRow = NormalizedJobRow;

interface ZhilianAdapterOptions {
  sessionDir: string;
  headless: boolean;
}

export function buildZhilianSearchUrl(params: CrawlJobsParams): string {
  const url = new URL("https://sou.zhaopin.com/");
  url.searchParams.set("kw", params.keyword.trim());
  if (params.city?.trim()) {
    url.searchParams.set("jl", params.city.trim());
  }
  if (params.page && params.page > 1) {
    url.searchParams.set("p", String(params.page));
  }
  return url.toString();
}

export function buildZhilianCandidatesUrl(params: CrawlCandidatesParams): string {
  const url = new URL("https://rd6.zhaopin.com/svip/recommend/candidate");
  url.searchParams.set("positionNumber", params.jobId.trim());
  if (params.page && params.page > 1) {
    url.searchParams.set("page", String(params.page));
  }
  return url.toString();
}

export function buildZhilianResumeUrl(candidateId: string): string {
  const url = new URL("https://rd6.zhaopin.com/resume/detail");
  url.searchParams.set("resumeNumber", candidateId.trim());
  return url.toString();
}

export function normalizeZhilianJobRows(rows: unknown[]): ZhilianJobRow[] {
  return normalizeJobRows("zhilian", rows);
}

export class ZhilianAdapter implements SourceAdapter {
  public readonly source = "zhilian" as const;

  private readonly jobSelectors: PageFieldSelectors = {
    cards: [
      ".positionlist__list-item",
      ".joblist-box__item",
      ".joblist-item",
      ".jobinfo-item",
      ".position-card",
    ],
    title: [".jobinfo__name", ".iteminfo__line1__jobname", ".position-name", "h3", "h4"],
    company: [".company__title", ".company__name", ".company-name", ".iteminfo__line2", ".company"],
    city: [".jobinfo__other-info", ".iteminfo__line2", ".job-area", ".city"],
    salary: [".jobinfo__salary", ".salary", ".iteminfo__line1__salary"],
    description: [".jobinfo__tag", ".iteminfo__line3", ".welfare", ".desc"],
    link: ["a[href*='jobs.zhaopin.com']", "a[href*='zhaopin.com']", "a"],
  };

  private readonly candidateSelectors: PageFieldSelectors = {
    cards: [
      ".candidate-card",
      ".resume-item",
      ".recommend-item",
      ".list-item",
      ".resume-list-item",
    ],
    name: [".name", ".candidate-name", ".resume-name", "h3", "h4"],
    company: [".company", ".company-name", ".work-company", ".current-company"],
    years: [".work-year", ".experience", ".years", ".resume-info"],
    tag: [".tag", ".status", ".label"],
    phone: [".phone", ".mobile", ".tel"],
    email: [".email"],
    link: ["a[href*='resume']", "a[href*='rd6.zhaopin.com']", "a"],
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

  constructor(private readonly options: ZhilianAdapterOptions) {}

  async checkSession(): Promise<{ valid: boolean; message?: string }> {
    try {
      return await withPersistentContext(this.options, async (context, page) => {
        await navigateAndStabilize(
          page,
          buildZhilianSearchUrl({
            keyword: "前端",
          }),
          "compliant",
        );
        const cookies = await context.cookies();
        const valid = cookies.some((cookie) => cookie.domain.includes("zhaopin.com"));

        return {
          valid,
          message: valid
            ? "Zhilian session detected from persistent browser profile"
            : "Zhilian session cookie not found, please login in persistent profile",
        };
      });
    } catch (error) {
      return {
        valid: false,
        message: error instanceof Error ? error.message : "Failed to check Zhilian session",
      };
    }
  }

  async crawlJobs(mode: CrawlMode, params: CrawlJobsParams): Promise<unknown[]> {
    return withPersistentContext(this.options, async (_context, page) => {
      const targetUrl = buildZhilianSearchUrl(params);
      await navigateAndStabilize(page, targetUrl, mode);
      await assertPageAvailable(page, this.source);

      const rawRows = await extractJobCards(page, this.jobSelectors);
      const normalized = normalizeZhilianJobRows(rawRows);
      if (normalized.length === 0) {
        throw new Error("zhilian_jobs_parse_empty");
      }
      return normalized;
    });
  }

  async crawlCandidates(mode: CrawlMode, params: CrawlCandidatesParams): Promise<unknown[]> {
    return withPersistentContext(this.options, async (_context, page) => {
      const targetUrl = buildZhilianCandidatesUrl(params);
      await navigateAndStabilize(page, targetUrl, mode);
      await assertPageAvailable(page, this.source);

      const rawRows = await extractCandidateCards(page, this.candidateSelectors);
      const normalized = normalizeCandidateRows(this.source, rawRows);
      if (normalized.length === 0) {
        throw new Error("zhilian_candidates_parse_empty");
      }
      return normalized;
    });
  }

  async crawlResume(
    mode: CrawlMode,
    candidateId: string,
  ): Promise<{ rawText: string; parsed: Record<string, unknown> }> {
    return withPersistentContext(this.options, async (_context, page) => {
      const targetUrl = resolveDetailUrl(candidateId, buildZhilianResumeUrl);
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
        throw new Error("zhilian_resume_normalize_failed");
      }
      return normalized;
    });
  }
}
