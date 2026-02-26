import type { CrawlMode } from "@doss/shared";
import type { CrawlCandidatesParams, CrawlJobsParams, SourceAdapter } from "./types";

interface MockRow {
  title: string;
  company: string;
  salaryK: string;
  city: string;
}

export class MockAdapter implements SourceAdapter {
  constructor(
    public readonly source: "boss" | "zhilian" | "wuba",
    private readonly sampleJobs: MockRow[],
  ) {}

  async checkSession(): Promise<{ valid: boolean; message?: string }> {
    return {
      valid: true,
      message: "Session available from local browser profile",
    };
  }

  async crawlJobs(mode: CrawlMode, params: CrawlJobsParams): Promise<unknown[]> {
    const suffix = mode === "advanced" ? "advanced" : "compliant";
    return this.sampleJobs.map((item, index) => ({
      externalId: `${this.source}-job-${suffix}-${index + 1}`,
      source: this.source,
      ...item,
      keyword: params.keyword,
    }));
  }

  async crawlCandidates(mode: CrawlMode, params: CrawlCandidatesParams): Promise<unknown[]> {
    const flag = mode === "advanced" ? "high-throughput" : "safe";
    return [
      {
        externalId: `${this.source}-candidate-1`,
        name: "示例候选人A",
        currentCompany: "示例科技",
        years: 5,
        tag: flag,
        jobId: params.jobId,
      },
      {
        externalId: `${this.source}-candidate-2`,
        name: "示例候选人B",
        currentCompany: "产品工作室",
        years: 7,
        tag: flag,
        jobId: params.jobId,
      },
    ];
  }

  async crawlResume(
    mode: CrawlMode,
    candidateId: string,
  ): Promise<{ rawText: string; parsed: Record<string, unknown> }> {
    const modeText = mode === "advanced" ? "advanced mode" : "compliant mode";
    const rawText = `${candidateId} resume text captured in ${modeText}. Focuses on Vue, TypeScript, data design and delivery.`;

    return {
      rawText,
      parsed: {
        candidateId,
        skills: ["Vue3", "TypeScript", "Playwright"],
        education: [{ school: "Demo University", degree: "Computer Science" }],
      },
    };
  }
}
