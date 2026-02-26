import type { CrawlMode, SourceType } from "@doss/shared";

export interface CrawlJobsParams {
  keyword: string;
  city?: string;
  page?: number;
}

export interface CrawlCandidatesParams {
  jobId: string;
  page?: number;
}

export interface SourceAdapter {
  readonly source: SourceType;
  checkSession(): Promise<{ valid: boolean; message?: string }>;
  crawlJobs(mode: CrawlMode, params: CrawlJobsParams): Promise<unknown[]>;
  crawlCandidates(mode: CrawlMode, params: CrawlCandidatesParams): Promise<unknown[]>;
  crawlResume(mode: CrawlMode, candidateId: string): Promise<{ rawText: string; parsed: Record<string, unknown> }>;
}
