import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const backend = vi.hoisted(() => ({
  checkSidecarHealth: vi.fn(),
  ensureSidecar: vi.fn(),
  createCandidate: vi.fn(),
  createCrawlTask: vi.fn(),
  createJob: vi.fn(),
  generateInterviewKit: vi.fn(),
  getAiProviderSettings: vi.fn(),
  getScreeningTemplate: vi.fn(),
  getTaskRuntimeSettings: vi.fn(),
  getHealth: vi.fn(),
  listAnalysis: vi.fn(),
  listCandidates: vi.fn(),
  listCrawlTasks: vi.fn(),
  listJobs: vi.fn(),
  listPipelineEvents: vi.fn(),
  listScreeningResults: vi.fn(),
  loadDashboardMetrics: vi.fn(),
  mergeCandidateImport: vi.fn(),
  moveCandidateStage: vi.fn(),
  runCandidateAnalysis: vi.fn(),
  runResumeScreening: vi.fn(),
  parseResumeFile: vi.fn(),
  runInterviewEvaluation: vi.fn(),
  searchCandidates: vi.fn(),
  saveInterviewKit: vi.fn(),
  submitInterviewFeedback: vi.fn(),
  testAiProviderSettings: vi.fn(),
  triggerSidecarCrawlCandidates: vi.fn(),
  triggerSidecarCrawlJobs: vi.fn(),
  triggerSidecarCrawlResume: vi.fn(),
  updateCrawlTask: vi.fn(),
  upsertAiProviderSettings: vi.fn(),
  upsertScreeningTemplate: vi.fn(),
  upsertTaskRuntimeSettings: vi.fn(),
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
    backend.getAiProviderSettings.mockResolvedValue({
      provider: "qwen",
      model: "qwen-plus-latest",
      base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
      temperature: 0.2,
      max_tokens: 1500,
      timeout_secs: 35,
      retry_count: 2,
      has_api_key: false,
    });
    backend.getTaskRuntimeSettings.mockResolvedValue({
      auto_batch_concurrency: 2,
      auto_retry_count: 1,
      auto_retry_backoff_ms: 200,
    });
    backend.getScreeningTemplate.mockResolvedValue({
      id: 1,
      scope: "global",
      name: "默认模板",
      job_id: null,
      dimensions: [],
      risk_rules: {},
      created_at: "2026-02-26T00:00:00Z",
      updated_at: "2026-02-26T00:00:00Z",
    });
    backend.ensureSidecar.mockResolvedValue({
      ok: true,
      port: 3791,
      base_url: "http://127.0.0.1:3791",
      source: "existing",
      restart_count: 0,
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
    backend.mergeCandidateImport.mockImplementation(async (input: { candidate_id: number; tags?: string[]; years_of_experience?: number; current_company?: string }) => ({
      id: input.candidate_id,
      source: "boss",
      external_id: "boss-existing",
      name: "张三",
      current_company: input.current_company ?? "示例科技",
      years_of_experience: input.years_of_experience ?? 5,
      stage: "NEW",
      tags: input.tags ?? ["safe"],
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
      model_info: { provider: "qwen", model: "qwen-plus-latest", generatedAt: "2026-02-26T00:00:00Z" },
      created_at: "2026-02-26T00:00:00Z",
    });
    backend.listAnalysis.mockResolvedValue([]);
    backend.listPipelineEvents.mockResolvedValue([]);
    backend.listScreeningResults.mockResolvedValue([]);
    backend.runResumeScreening.mockResolvedValue({
      id: 1,
      candidate_id: 11,
      job_id: 101,
      t0_score: 4.1,
      t1_score: 78,
      fine_score: 71,
      bonus_score: 6,
      risk_penalty: 5,
      overall_score: 77,
      recommendation: "REVIEW",
      risk_level: "MEDIUM",
      evidence: ["技能匹配命中 3 项"],
      verification_points: ["补充项目深度验证"],
      created_at: "2026-02-26T00:00:00Z",
    });
    backend.parseResumeFile.mockResolvedValue({
      raw_text: "候选人简历文本",
      parsed: {
        skills: ["Vue3", "TypeScript"],
        expectedSalaryK: 45,
      },
      metadata: {
        fileName: "resume.pdf",
        extension: "pdf",
      },
    });
    backend.generateInterviewKit.mockResolvedValue({
      id: null,
      candidate_id: 11,
      job_id: 101,
      questions: [
        {
          primary_question: "请复盘一个项目。",
          follow_ups: ["你的关键决策是什么？"],
          scoring_points: ["结构清晰", "结果量化"],
          red_flags: ["回避关键指标"],
        },
      ],
      generated_by: "rule-engine-v1",
      created_at: "2026-02-26T00:00:00Z",
      updated_at: "2026-02-26T00:00:00Z",
    });
    backend.saveInterviewKit.mockResolvedValue({
      id: 1,
      candidate_id: 11,
      job_id: 101,
      questions: [
        {
          primary_question: "请复盘一个项目。",
          follow_ups: ["你的关键决策是什么？"],
          scoring_points: ["结构清晰", "结果量化"],
          red_flags: ["回避关键指标"],
        },
      ],
      generated_by: "rule-engine-v1",
      created_at: "2026-02-26T00:00:00Z",
      updated_at: "2026-02-26T00:00:00Z",
    });
    backend.submitInterviewFeedback.mockResolvedValue({
      id: 1,
      candidate_id: 11,
      job_id: 101,
      transcript_text: "问题回答较少",
      structured_feedback: {
        scores: {
          communication: 3,
        },
      },
      recording_path: null,
      created_at: "2026-02-26T00:00:00Z",
      updated_at: "2026-02-26T00:00:00Z",
    });
    backend.runInterviewEvaluation.mockResolvedValue({
      id: 1,
      candidate_id: 11,
      job_id: 101,
      feedback_id: 1,
      recommendation: "HOLD",
      overall_score: 61,
      confidence: 0.42,
      evidence: ["转写不足"],
      verification_points: ["补充面试记录"],
      uncertainty: "证据不足",
      created_at: "2026-02-26T00:00:00Z",
    });
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
    expect(result.mergedCandidates).toBe(0);
    expect(result.conflictCandidates).toBe(0);
    expect(result.skippedCandidates).toBe(0);
    expect(result.resumeAutoProcessed).toBe(2);
    expect(result.analysisTriggered).toBe(2);

    expect(backend.triggerSidecarCrawlResume).toHaveBeenCalledTimes(2);
    expect(backend.runCandidateAnalysis).toHaveBeenCalledTimes(2);
  });

  it("marks crawl task failed with sidecar error code and snapshot", async () => {
    backend.triggerSidecarCrawlCandidates.mockResolvedValue({
      id: "sidecar-candidate-task",
      source: "boss",
      mode: "compliant",
      status: "FAILED",
      attempts: 3,
      error: "Navigation timeout of 20000 ms exceeded",
      errorCode: "TIMEOUT",
      snapshot: {
        source: "boss",
        taskType: "candidates",
      },
    });

    const store = useRecruitingStore();
    await store.bootstrap();

    await expect(
      store.runSidecarCandidateCrawl({
        source: "boss",
        mode: "compliant",
        localJobId: 101,
      }),
    ).rejects.toThrow("Navigation timeout of 20000 ms exceeded");

    expect(backend.updateCrawlTask).toHaveBeenCalledWith(
      expect.objectContaining({
        status: "FAILED",
        error_code: "TIMEOUT",
        snapshot: {
          source: "boss",
          taskType: "candidates",
        },
      }),
    );
  });

  it("cross-source merge candidates and records quality report", async () => {
    backend.listCandidates.mockResolvedValue([
      {
        id: 22,
        source: "boss",
        external_id: "boss-existing-22",
        name: "张三",
        current_company: "示例科技",
        years_of_experience: 5,
        stage: "NEW",
        tags: ["safe"],
        created_at: "2026-02-26T00:00:00Z",
        updated_at: "2026-02-26T00:00:00Z",
      },
    ]);
    backend.triggerSidecarCrawlCandidates.mockResolvedValue({
      id: "sidecar-candidate-task",
      source: "zhilian",
      mode: "compliant",
      status: "SUCCEEDED",
      attempts: 1,
      output: [
        {
          externalId: "zhilian-candidate-9",
          name: "张三",
          currentCompany: "示例科技",
          years: 6,
          tag: "stable",
        },
      ],
    });

    const store = useRecruitingStore();
    await store.bootstrap();

    const result = await store.runSidecarCandidateCrawl({
      source: "zhilian",
      mode: "compliant",
      localJobId: 101,
    });

    expect(result.importedCandidates).toBe(0);
    expect(result.mergedCandidates).toBe(1);
    expect(result.conflictCandidates).toBe(0);
    expect(backend.mergeCandidateImport).toHaveBeenCalledTimes(1);
    expect(store.lastCandidateImportReport?.mergedRows).toBe(1);
  });

  it("keeps conflicts for manual confirmation and supports resolving", async () => {
    backend.listCandidates.mockResolvedValue([
      {
        id: 33,
        source: "boss",
        external_id: "boss-existing-33",
        name: "王五",
        current_company: "示例科技",
        years_of_experience: 2,
        stage: "NEW",
        tags: ["safe"],
        created_at: "2026-02-26T00:00:00Z",
        updated_at: "2026-02-26T00:00:00Z",
      },
    ]);
    backend.triggerSidecarCrawlCandidates.mockResolvedValue({
      id: "sidecar-candidate-task",
      source: "zhilian",
      mode: "compliant",
      status: "SUCCEEDED",
      attempts: 1,
      output: [
        {
          externalId: "zhilian-candidate-33",
          name: "王五",
          currentCompany: "示例科技",
          years: 9,
          tag: "risky",
        },
      ],
    });

    const store = useRecruitingStore();
    await store.bootstrap();

    const result = await store.runSidecarCandidateCrawl({
      source: "zhilian",
      mode: "compliant",
      localJobId: 101,
    });

    expect(result.conflictCandidates).toBe(1);
    expect(store.candidateImportConflicts.length).toBe(1);

    const [conflict] = store.candidateImportConflicts;
    await store.resolveCandidateImportConflict({
      conflictId: conflict.id,
      action: "create",
    });
    expect(store.candidateImportConflicts.length).toBe(0);
    expect(backend.createCandidate).toHaveBeenCalled();
  });

  it("imports resume file and triggers analysis", async () => {
    const store = useRecruitingStore();
    await store.bootstrap();

    const file = new File(["resume content"], "resume.pdf", { type: "application/pdf" });
    await store.importResumeFileAndAnalyze({
      candidateId: 11,
      file,
      enableOcr: true,
    });

    expect(backend.parseResumeFile).toHaveBeenCalledWith(
      expect.objectContaining({
        file_name: "resume.pdf",
        enable_ocr: true,
      }),
    );
    expect(backend.upsertResume).toHaveBeenCalledWith(
      expect.objectContaining({
        candidate_id: 11,
        raw_text: "候选人简历文本",
      }),
    );
    expect(backend.runResumeScreening).toHaveBeenCalledWith(
      expect.objectContaining({
        candidate_id: 11,
      }),
    );
    expect(backend.runCandidateAnalysis).toHaveBeenCalledWith(
      expect.objectContaining({
        candidate_id: 11,
      }),
    );
  });

  it("supports task lifecycle actions and runtime setting update", async () => {
    backend.upsertTaskRuntimeSettings.mockResolvedValue({
      auto_batch_concurrency: 4,
      auto_retry_count: 2,
      auto_retry_backoff_ms: 600,
    });

    const store = useRecruitingStore();
    await store.bootstrap();

    await store.pauseTask(101);
    await store.resumeTask(101);
    await store.cancelTask(101);
    await store.saveTaskSettings({
      auto_batch_concurrency: 4,
      auto_retry_count: 2,
      auto_retry_backoff_ms: 600,
    });

    expect(backend.updateCrawlTask).toHaveBeenCalledWith(
      expect.objectContaining({ task_id: 101, status: "PAUSED" }),
    );
    expect(backend.updateCrawlTask).toHaveBeenCalledWith(
      expect.objectContaining({ task_id: 101, status: "PENDING" }),
    );
    expect(backend.updateCrawlTask).toHaveBeenCalledWith(
      expect.objectContaining({ task_id: 101, status: "CANCELED" }),
    );
    expect(backend.upsertTaskRuntimeSettings).toHaveBeenCalledWith(
      expect.objectContaining({ auto_batch_concurrency: 4 }),
    );
    expect(store.taskSettings?.auto_batch_concurrency).toBe(4);
  });

  it("stores HOLD evaluation when interview evidence is insufficient", async () => {
    const store = useRecruitingStore();
    await store.bootstrap();

    const kit = await store.generateInterviewKit(11, 101);
    await store.saveInterviewKit({
      candidate_id: 11,
      job_id: 101,
      questions: kit.questions,
    });
    await store.submitInterviewFeedback({
      candidate_id: 11,
      job_id: 101,
      transcript_text: "问题回答较少",
      structured_feedback: {
        scores: {
          communication: 3,
        },
      },
    });
    const evaluation = await store.runInterviewEvaluation({
      candidate_id: 11,
      job_id: 101,
    });

    expect(evaluation.recommendation).toBe("HOLD");
    expect(store.interviewEvaluations[11][0].verification_points[0]).toContain("补充");
  });
});
