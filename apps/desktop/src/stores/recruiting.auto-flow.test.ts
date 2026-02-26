import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const backend = vi.hoisted(() => ({
  checkSidecarHealth: vi.fn(),
  createCandidate: vi.fn(),
  createCrawlTask: vi.fn(),
  createJob: vi.fn(),
  getHealth: vi.fn(),
  listAnalysis: vi.fn(),
  listCandidates: vi.fn(),
  listCrawlTasks: vi.fn(),
  listJobs: vi.fn(),
  listPipelineEvents: vi.fn(),
  loadDashboardMetrics: vi.fn(),
  moveCandidateStage: vi.fn(),
  runCandidateAnalysis: vi.fn(),
  searchCandidates: vi.fn(),
  triggerSidecarCrawlCandidates: vi.fn(),
  triggerSidecarCrawlJobs: vi.fn(),
  triggerSidecarCrawlResume: vi.fn(),
  updateCrawlTask: vi.fn(),
  upsertResume: vi.fn(),
}));

vi.mock("../services/backend", () => backend);

import { useRecruitingStore } from "./recruiting";

describe("recruiting store auto workflow", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();

    backend.listJobs.mockResolvedValue([
      {
        id: 101,
        source: "boss",
        title: "前端工程师",
        company: "示例科技",
        created_at: "2026-02-26T00:00:00Z",
        updated_at: "2026-02-26T00:00:00Z",
      },
    ]);

    backend.listCandidates.mockResolvedValue([]);
    backend.listCrawlTasks.mockResolvedValue([]);
    backend.loadDashboardMetrics.mockResolvedValue({
      total_jobs: 1,
      total_candidates: 0,
      total_resumes: 0,
      pending_tasks: 0,
      stage_stats: [],
    });
    backend.getHealth.mockResolvedValue({
      ok: true,
      dbPath: "/tmp/test.sqlite",
      dbExists: true,
      schemaVersion: 0,
    });
    backend.checkSidecarHealth.mockResolvedValue({ ok: true });

    let nextTaskId = 1;
    backend.createCrawlTask.mockImplementation(async (input: { task_type: string }) => ({
      id: nextTaskId++,
      source: "boss",
      mode: "compliant",
      task_type: input.task_type,
      status: "PENDING",
      retry_count: 0,
      payload: {},
      created_at: "2026-02-26T00:00:00Z",
      updated_at: "2026-02-26T00:00:00Z",
    }));
    backend.updateCrawlTask.mockImplementation(async () => ({
      id: 1,
      source: "boss",
      mode: "compliant",
      task_type: "crawl_candidates",
      status: "SUCCEEDED",
      retry_count: 0,
      payload: {},
      created_at: "2026-02-26T00:00:00Z",
      updated_at: "2026-02-26T00:00:00Z",
    }));

    backend.triggerSidecarCrawlCandidates.mockResolvedValue({
      id: "sidecar-candidate-task",
      source: "boss",
      mode: "compliant",
      status: "SUCCEEDED",
      attempts: 1,
      output: [
        {
          externalId: "boss-candidate-1",
          name: "张三",
          currentCompany: "示例科技",
          years: 5,
          tag: "safe",
        },
        {
          externalId: "boss-candidate-2",
          name: "李四",
          currentCompany: "示例科技",
          years: 3,
          tag: "safe",
        },
      ],
    });

    let nextCandidateId = 11;
    backend.createCandidate.mockImplementation(async (input: { external_id?: string; name: string; years_of_experience: number; tags: string[] }) => ({
      id: nextCandidateId++,
      source: "boss",
      external_id: input.external_id,
      name: input.name,
      current_company: "示例科技",
      years_of_experience: input.years_of_experience,
      stage: "NEW",
      tags: input.tags,
      created_at: "2026-02-26T00:00:00Z",
      updated_at: "2026-02-26T00:00:00Z",
    }));

    backend.triggerSidecarCrawlResume.mockResolvedValue({
      id: "sidecar-resume-task",
      source: "boss",
      mode: "compliant",
      status: "SUCCEEDED",
      attempts: 1,
      output: {
        rawText: "resume text from sidecar",
        parsed: {
          skills: ["Vue3", "TypeScript"],
          expectedSalaryK: 45,
        },
      },
    });

    backend.upsertResume.mockResolvedValue({
      id: 1,
      candidate_id: 11,
      source: "manual",
      raw_text: "resume text from sidecar",
      parsed: {
        skills: ["Vue3", "TypeScript"],
      },
      created_at: "2026-02-26T00:00:00Z",
      updated_at: "2026-02-26T00:00:00Z",
    });
    backend.runCandidateAnalysis.mockResolvedValue({
      id: 1,
      candidate_id: 11,
      overall_score: 82,
      dimension_scores: [],
      risks: [],
      highlights: [],
      suggestions: [],
      evidence: [],
      model_info: { provider: "mock", model: "mock", generatedAt: "2026-02-26T00:00:00Z" },
      created_at: "2026-02-26T00:00:00Z",
    });
    backend.listAnalysis.mockResolvedValue([]);
    backend.listPipelineEvents.mockResolvedValue([]);
  });

  it("auto-fetches resumes and triggers analysis after candidate import", async () => {
    const store = useRecruitingStore();
    await store.bootstrap();

    const result = await store.runSidecarCandidateCrawl({
      source: "boss",
      mode: "compliant",
      localJobId: 101,
    });

    expect(result.importedCandidates).toBe(2);
    expect(result.resumeAutoProcessed).toBe(2);
    expect(result.analysisTriggered).toBe(2);

    expect(backend.triggerSidecarCrawlResume).toHaveBeenCalledTimes(2);
    expect(backend.runCandidateAnalysis).toHaveBeenCalledTimes(2);
  });
});
