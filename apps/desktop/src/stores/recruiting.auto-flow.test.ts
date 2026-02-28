import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const backend = vi.hoisted(() => ({
  checkSidecarHealth: vi.fn(),
  ensureSidecar: vi.fn(),
  createCandidate: vi.fn(),
  createCrawlTask: vi.fn(),
  createJob: vi.fn(),
  createScreeningTemplate: vi.fn(),
  deleteCrawlTask: vi.fn(),
  deleteJob: vi.fn(),
  deleteScreeningTemplate: vi.fn(),
  finalizeHiringDecision: vi.fn(),
  generateInterviewKit: vi.fn(),
  getAiProviderSettings: vi.fn(),
  getScreeningTemplate: vi.fn(),
  getTaskRuntimeSettings: vi.fn(),
  getHealth: vi.fn(),
  listAnalysis: vi.fn(),
  listHiringDecisions: vi.fn(),
  listInterviewEvaluations: vi.fn(),
  listCandidates: vi.fn(),
  listCrawlTasks: vi.fn(),
  listCrawlTaskPeople: vi.fn(),
  listJobs: vi.fn(),
  listPipelineEvents: vi.fn(),
  listScreeningTemplates: vi.fn(),
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
  setJobScreeningTemplate: vi.fn(),
  stopJob: vi.fn(),
  testAiProviderSettings: vi.fn(),
  triggerSidecarCrawlCandidates: vi.fn(),
  triggerSidecarCrawlJobs: vi.fn(),
  triggerSidecarCrawlResume: vi.fn(),
  updateCandidate: vi.fn(),
  updateCrawlTask: vi.fn(),
  updateCrawlTaskPeopleSync: vi.fn(),
  updateJob: vi.fn(),
  updateScreeningTemplate: vi.fn(),
  upsertAiProviderSettings: vi.fn(),
  setCandidateQualification: vi.fn(),
  upsertPendingCandidates: vi.fn(),
  syncPendingCandidateToCandidate: vi.fn(),
  upsertScreeningTemplate: vi.fn(),
  upsertTaskRuntimeSettings: vi.fn(),
  upsertCrawlTaskPeople: vi.fn(),
  deleteCandidate: vi.fn(),
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
    backend.listCrawlTaskPeople.mockResolvedValue([]);
    backend.upsertCrawlTaskPeople.mockResolvedValue([]);
    backend.updateCrawlTaskPeopleSync.mockResolvedValue([]);
    backend.upsertPendingCandidates.mockResolvedValue([]);
    backend.syncPendingCandidateToCandidate.mockResolvedValue({
      id: 11,
      source: "boss",
      external_id: "boss-existing-11",
      name: "张三",
      current_company: "示例科技",
      job_id: 101,
      job_title: "前端工程师",
      years_of_experience: 5,
      stage: "NEW",
      tags: ["safe"],
      created_at: "2026-02-26T00:00:00Z",
      updated_at: "2026-02-26T00:00:00Z",
    });
    backend.deleteCrawlTask.mockResolvedValue(true);
    backend.loadDashboardMetrics.mockResolvedValue({
      total_jobs: 1,
      total_candidates: 0,
      total_resumes: 0,
      pending_tasks: 0,
      hiring_decisions_total: 0,
      ai_alignment_count: 0,
      ai_deviation_count: 0,
      ai_alignment_rate: 0,
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
    backend.listScreeningTemplates.mockResolvedValue([
      {
        id: 1,
        scope: "global",
        name: "默认模板",
        job_id: null,
        dimensions: [
          {
            key: "goal_orientation",
            label: "目标导向",
            weight: 100,
          },
        ],
        risk_rules: {},
        created_at: "2026-02-26T00:00:00Z",
        updated_at: "2026-02-26T00:00:00Z",
      },
    ]);
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
    backend.listHiringDecisions.mockResolvedValue([]);
    backend.listInterviewEvaluations.mockResolvedValue([]);
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
    backend.finalizeHiringDecision.mockResolvedValue({
      id: 1,
      candidate_id: 11,
      job_id: 101,
      interview_evaluation_id: 1,
      ai_recommendation: "HOLD",
      final_decision: "HIRE",
      reason_code: "skills_match",
      note: "业务强相关经验",
      ai_deviation: true,
      created_at: "2026-02-26T00:00:00Z",
      updated_at: "2026-02-26T00:00:00Z",
    });
    backend.updateJob.mockResolvedValue({
      id: 101,
      source: "boss",
      title: "高级前端工程师",
      company: "示例科技",
      city: "杭州",
      salary_k: "35-50",
      description: "Vue3 + TS + Playwright",
      status: "ACTIVE",
      created_at: "2026-02-26T00:00:00Z",
      updated_at: "2026-02-26T00:00:00Z",
    });
    backend.stopJob.mockResolvedValue({
      id: 101,
      source: "boss",
      title: "高级前端工程师",
      company: "示例科技",
      city: "杭州",
      salary_k: "35-50",
      description: "Vue3 + TS + Playwright",
      status: "STOPPED",
      created_at: "2026-02-26T00:00:00Z",
      updated_at: "2026-02-26T00:00:00Z",
    });
    backend.deleteJob.mockResolvedValue(true);
    backend.setJobScreeningTemplate.mockResolvedValue({
      id: 101,
      source: "boss",
      title: "高级前端工程师",
      company: "示例科技",
      city: "杭州",
      salary_k: "35-50",
      description: "Vue3 + TS + Playwright",
      status: "ACTIVE",
      screening_template_id: 1,
      screening_template_name: "默认模板",
      created_at: "2026-02-26T00:00:00Z",
      updated_at: "2026-02-26T00:00:00Z",
    });
    backend.createScreeningTemplate.mockResolvedValue({
      id: 9,
      scope: "global",
      name: "前端模板",
      job_id: null,
      dimensions: [
        {
          key: "goal_orientation",
          label: "目标导向",
          weight: 100,
        },
      ],
      risk_rules: {},
      created_at: "2026-02-26T00:00:00Z",
      updated_at: "2026-02-26T00:00:00Z",
    });
    backend.updateScreeningTemplate.mockResolvedValue({
      id: 9,
      scope: "global",
      name: "前端模板v2",
      job_id: null,
      dimensions: [
        {
          key: "goal_orientation",
          label: "目标导向",
          weight: 100,
        },
      ],
      risk_rules: {},
      created_at: "2026-02-26T00:00:00Z",
      updated_at: "2026-02-26T00:00:00Z",
    });
    backend.deleteScreeningTemplate.mockResolvedValue([
      {
        id: 1,
        scope: "global",
        name: "默认模板",
        job_id: null,
        dimensions: [
          {
            key: "goal_orientation",
            label: "目标导向",
            weight: 100,
          },
        ],
        risk_rules: {},
        created_at: "2026-02-26T00:00:00Z",
        updated_at: "2026-02-26T00:00:00Z",
      },
    ]);
  });

  it("captures ensure_sidecar failures for diagnostics", async () => {
    backend.ensureSidecar.mockRejectedValueOnce(new Error("sidecar_port_conflict"));

    const store = useRecruitingStore();
    await store.refreshSidecarHealth();

    expect(store.sidecarHealthy).toBe(false);
    expect(store.sidecarError).toBe("sidecar_port_conflict");
    expect(backend.checkSidecarHealth).not.toHaveBeenCalled();
  });

  it("polls sidecar health by interval and supports stop", async () => {
    vi.useFakeTimers();
    const store = useRecruitingStore();

    try {
      store.startSidecarHealthPolling(1_000);
      await vi.advanceTimersByTimeAsync(3_100);

      expect(backend.ensureSidecar).toHaveBeenCalledTimes(3);
      expect(backend.checkSidecarHealth).toHaveBeenCalledTimes(3);

      store.stopSidecarHealthPolling();
      const ensureCalls = backend.ensureSidecar.mock.calls.length;

      await vi.advanceTimersByTimeAsync(2_100);
      expect(backend.ensureSidecar).toHaveBeenCalledTimes(ensureCalls);
    } finally {
      store.stopSidecarHealthPolling();
      vi.useRealTimers();
    }
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
    expect(backend.upsertResume).toHaveBeenCalledTimes(2);
    for (const [payload] of backend.upsertResume.mock.calls) {
      expect(payload).toEqual(expect.objectContaining({
        source: "boss",
      }));
    }
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

  it("does not merge when same-name candidates have different age and both ages are present", async () => {
    backend.listCandidates.mockResolvedValue([
      {
        id: 52,
        source: "boss",
        external_id: "boss-existing-52",
        name: "赵敏",
        current_company: "示例科技",
        age: 30,
        address: "上海",
        years_of_experience: 7,
        stage: "NEW",
        tags: ["safe"],
        created_at: "2026-02-26T00:00:00Z",
        updated_at: "2026-02-26T00:00:00Z",
      },
    ]);
    backend.triggerSidecarCrawlCandidates.mockResolvedValue({
      id: "sidecar-candidate-task",
      source: "lagou",
      mode: "compliant",
      status: "SUCCEEDED",
      attempts: 1,
      output: [
        {
          externalId: "lagou-candidate-52",
          name: "赵敏",
          currentCompany: "示例科技",
          age: 29,
          address: "上海",
          years: 6,
          tag: "safe",
        },
      ],
    });

    const store = useRecruitingStore();
    await store.bootstrap();

    const result = await store.runSidecarCandidateCrawl({
      source: "lagou",
      mode: "compliant",
      localJobId: 101,
    });

    expect(result.importedCandidates).toBe(1);
    expect(result.mergedCandidates).toBe(0);
    expect(backend.createCandidate).toHaveBeenCalledWith(expect.objectContaining({
      name: "赵敏",
      age: 29,
      address: "上海",
    }));
    expect(backend.mergeCandidateImport).not.toHaveBeenCalled();
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
      {
        id: 34,
        source: "lagou",
        external_id: "lagou-existing-34",
        name: "王五",
        current_company: "另一家公司",
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
        source: "manual",
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
    expect(backend.loadDashboardMetrics).toHaveBeenCalledTimes(2);
  });

  it("imports resume file without triggering screening or analysis", async () => {
    const store = useRecruitingStore();
    await store.bootstrap();

    const file = new File(["resume content"], "resume.pdf", { type: "application/pdf" });
    await store.importResumeFile({
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
        source: "manual",
        raw_text: "候选人简历文本",
      }),
    );
    expect(backend.runResumeScreening).not.toHaveBeenCalled();
    expect(backend.runCandidateAnalysis).not.toHaveBeenCalled();
    expect(backend.loadDashboardMetrics).toHaveBeenCalledTimes(2);
  });

  it("reruns AI analysis without triggering screening", async () => {
    const store = useRecruitingStore();
    await store.bootstrap();

    await store.rerunAiAnalysis(11, 101);

    expect(backend.runCandidateAnalysis).toHaveBeenCalledWith({
      candidate_id: 11,
      job_id: 101,
    });
    expect(backend.runResumeScreening).not.toHaveBeenCalled();
    expect(backend.listAnalysis).toHaveBeenCalledWith(11);
  });

  it("falls back cleanly when search query causes backend error", async () => {
    backend.searchCandidates
      .mockResolvedValueOnce([
        {
          candidate_id: 11,
          name: "张三",
          stage: "NEW",
          snippet: "Vue3",
        },
      ])
      .mockRejectedValueOnce(new Error("fts_parse_error"));

    const store = useRecruitingStore();
    await store.bootstrap();

    await store.search("Vue3");
    expect(store.searchResults).toHaveLength(1);

    await expect(store.search("\"")).resolves.toBeDefined();
    expect(store.searchResults).toEqual([]);
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

  it("creates crawl_candidates task as pending with task-level payload", async () => {
    backend.createCrawlTask.mockImplementationOnce(async (input: {
      source: "all" | "boss" | "zhilian" | "wuba" | "lagou";
      mode: "compliant" | "advanced";
      task_type: string;
      payload: Record<string, unknown>;
    }) => ({
      id: 201,
      source: input.source,
      mode: input.mode,
      task_type: input.task_type,
      status: "PENDING",
      retry_count: 0,
      payload: input.payload,
      created_at: "2026-02-26T00:00:00Z",
      updated_at: "2026-02-26T00:00:00Z",
    }));

    const store = useRecruitingStore();
    await store.bootstrap();

    await store.createCandidatesTask({
      source: "all",
      mode: "compliant",
      localJobId: 101,
      batchSize: 50,
      scheduleType: "DAILY",
      scheduleTime: "09:30",
      scheduleDay: 15,
      retryCount: 1,
      retryBackoffMs: 450,
      autoSyncToCandidates: true,
    });

    expect(backend.createCrawlTask).toHaveBeenCalledWith(expect.objectContaining({
      source: "all",
      mode: "compliant",
      task_type: "crawl_candidates",
      schedule_type: "DAILY",
      schedule_time: "09:30",
      schedule_day: 15,
      next_run_at: expect.any(String),
      payload: expect.objectContaining({
        localJobId: 101,
        localJobTitle: "前端工程师",
        batchSize: 50,
        scheduleType: "DAILY",
        scheduleTime: "09:30",
        scheduleDay: 15,
        retryCount: 1,
        retryBackoffMs: 450,
        autoSyncToCandidates: true,
      }),
    }));
    expect(store.tasks[0]?.status).toBe("PENDING");
  });

  it("toggles pending task to running and executes one cycle immediately", async () => {
    const store = useRecruitingStore();
    await store.bootstrap();

    store.tasks = [
      {
        id: 301,
        source: "all",
        mode: "compliant",
        task_type: "crawl_candidates",
        status: "PENDING",
        retry_count: 0,
        payload: {
          localJobId: 101,
          localJobTitle: "前端工程师",
          localJobCity: "杭州",
          batchSize: 1,
          crawlIntervalSeconds: 300,
          retryCount: 0,
          retryBackoffMs: 200,
          autoSyncToCandidates: false,
        },
        created_at: "2026-02-26T00:00:00Z",
        updated_at: "2026-02-26T00:00:00Z",
      },
    ];

    backend.listCrawlTasks
      .mockResolvedValueOnce([
        {
          ...store.tasks[0],
          status: "RUNNING",
        },
      ])
      .mockResolvedValueOnce([
        {
          ...store.tasks[0],
          status: "PAUSED",
        },
      ]);
    backend.triggerSidecarCrawlJobs.mockResolvedValue({
      id: "sidecar-job-task",
      source: "boss",
      mode: "compliant",
      status: "SUCCEEDED",
      attempts: 1,
      output: [{
        externalId: "job-1",
        title: "前端工程师",
        company: "示例科技",
        city: "杭州",
      }],
    });
    backend.triggerSidecarCrawlCandidates.mockResolvedValue({
      id: "sidecar-candidate-task",
      source: "boss",
      mode: "compliant",
      status: "SUCCEEDED",
      attempts: 1,
      output: [{
        externalId: "boss-candidate-301",
        name: "赵六",
        currentCompany: "示例科技",
        years: 4,
      }],
    });
    backend.upsertCrawlTaskPeople.mockResolvedValue([
      {
        id: 1,
        task_id: 301,
        source: "boss",
        external_id: "boss-candidate-301",
        name: "赵六",
        current_company: "示例科技",
        years_of_experience: 4,
        sync_status: "UNSYNCED",
        sync_error_code: null,
        sync_error_message: null,
        candidate_id: null,
        created_at: "2026-02-26T00:00:00Z",
        updated_at: "2026-02-26T00:00:00Z",
      },
    ]);

    await store.toggleTaskRunState(301);

    expect(backend.updateCrawlTask).toHaveBeenCalledWith(expect.objectContaining({
      task_id: 301,
      status: "RUNNING",
    }));
    await vi.waitFor(() => {
      expect(backend.updateCrawlTask).toHaveBeenCalledWith(expect.objectContaining({
        task_id: 301,
        status: "RUNNING",
        snapshot: expect.objectContaining({
          fetchedPeople: 1,
        }),
      }));
      expect(backend.upsertCrawlTaskPeople).toHaveBeenCalledWith(expect.objectContaining({
        task_id: 301,
      }));
      expect(backend.upsertPendingCandidates).toHaveBeenCalledWith(expect.objectContaining({
        items: [expect.objectContaining({
          source: "boss",
          external_id: "boss-candidate-301",
          name: "赵六",
          job_id: 101,
        })],
      }));
    });
  });

  it("does not block start action when crawl cycle is slow", async () => {
    const store = useRecruitingStore();
    await store.bootstrap();

    store.tasks = [
      {
        id: 304,
        source: "boss",
        mode: "compliant",
        task_type: "crawl_candidates",
        status: "PENDING",
        retry_count: 0,
        payload: {
          localJobId: 101,
          localJobTitle: "前端工程师",
          localJobCity: "杭州",
          batchSize: 1,
          crawlIntervalSeconds: 300,
          retryCount: 0,
          retryBackoffMs: 200,
          autoSyncToCandidates: false,
        },
        created_at: "2026-02-26T00:00:00Z",
        updated_at: "2026-02-26T00:00:00Z",
      },
    ];

    backend.listCrawlTasks.mockResolvedValueOnce([
      {
        ...store.tasks[0],
        status: "RUNNING",
      },
    ]);
    backend.triggerSidecarCrawlJobs.mockImplementation(
      () =>
        new Promise(() => {
          // Keep pending to simulate a slow sidecar request.
        }),
    );

    const startPromise = store.toggleTaskRunState(304);
    const result = await Promise.race([
      startPromise.then(() => "resolved"),
      new Promise<"timeout">((resolve) => setTimeout(() => resolve("timeout"), 30)),
    ]);

    expect(result).toBe("resolved");
    expect(backend.updateCrawlTask).toHaveBeenCalledWith(expect.objectContaining({
      task_id: 304,
      status: "RUNNING",
    }));
  });

  it("inserts pending candidates first and syncs candidates when auto sync is enabled", async () => {
    const store = useRecruitingStore();
    await store.bootstrap();

    store.tasks = [
      {
        id: 303,
        source: "boss",
        mode: "compliant",
        task_type: "crawl_candidates",
        status: "PENDING",
        retry_count: 0,
        payload: {
          localJobId: 101,
          localJobTitle: "前端工程师",
          localJobCity: "杭州",
          batchSize: 1,
          crawlIntervalSeconds: 300,
          retryCount: 0,
          retryBackoffMs: 200,
          autoSyncToCandidates: true,
        },
        created_at: "2026-02-26T00:00:00Z",
        updated_at: "2026-02-26T00:00:00Z",
      },
    ];

    backend.listCrawlTasks
      .mockResolvedValueOnce([
        {
          ...store.tasks[0],
          status: "RUNNING",
        },
      ])
      .mockResolvedValueOnce([
        {
          ...store.tasks[0],
          status: "PAUSED",
        },
      ]);
    backend.triggerSidecarCrawlJobs.mockResolvedValue({
      id: "sidecar-job-task",
      source: "boss",
      mode: "compliant",
      status: "SUCCEEDED",
      attempts: 1,
      output: [{
        externalId: "job-1",
        title: "前端工程师",
        company: "示例科技",
        city: "杭州",
      }],
    });
    backend.triggerSidecarCrawlCandidates.mockResolvedValue({
      id: "sidecar-candidate-task",
      source: "boss",
      mode: "compliant",
      status: "SUCCEEDED",
      attempts: 1,
      output: [{
        externalId: "boss-candidate-303",
        name: "钱七",
        currentCompany: "示例科技",
        years: 4,
        age: 28,
        address: "上海",
        tag: "safe",
      }],
    });
    backend.upsertCrawlTaskPeople.mockResolvedValue([
      {
        id: 2,
        task_id: 303,
        source: "boss",
        external_id: "boss-candidate-303",
        name: "钱七",
        current_company: "示例科技",
        years_of_experience: 4,
        sync_status: "UNSYNCED",
        sync_error_code: null,
        sync_error_message: null,
        candidate_id: null,
        created_at: "2026-02-26T00:00:00Z",
        updated_at: "2026-02-26T00:00:00Z",
      },
    ]);
    backend.upsertPendingCandidates.mockResolvedValue([
      {
        id: 801,
        source: "boss",
        external_id: "boss-candidate-303",
        name: "钱七",
        current_company: "示例科技",
        job_id: 101,
        job_title: "前端工程师",
        age: 28,
        years_of_experience: 4,
        address: "上海",
        tags: ["safe"],
        dedupe_key: "钱七|28|上海",
        sync_status: "UNSYNCED",
        candidate_id: null,
        resume_parsed: {},
        created_at: "2026-02-26T00:00:00Z",
        updated_at: "2026-02-26T00:00:00Z",
      },
    ]);
    backend.syncPendingCandidateToCandidate.mockResolvedValue({
      id: 11,
      source: "boss",
      external_id: "boss-candidate-303",
      name: "钱七",
      current_company: "示例科技",
      job_id: 101,
      job_title: "前端工程师",
      years_of_experience: 4,
      stage: "NEW",
      tags: ["safe"],
      created_at: "2026-02-26T00:00:00Z",
      updated_at: "2026-02-26T00:00:00Z",
    });
    backend.updateCrawlTaskPeopleSync.mockResolvedValue([
      {
        id: 2,
        task_id: 303,
        source: "boss",
        external_id: "boss-candidate-303",
        name: "钱七",
        current_company: "示例科技",
        years_of_experience: 4,
        sync_status: "SYNCED",
        sync_error_code: null,
        sync_error_message: null,
        candidate_id: 11,
        created_at: "2026-02-26T00:00:00Z",
        updated_at: "2026-02-26T00:00:00Z",
      },
    ]);

    await store.toggleTaskRunState(303);

    await vi.waitFor(() => {
      expect(backend.upsertPendingCandidates).toHaveBeenCalledWith({
        items: [expect.objectContaining({
          source: "boss",
          external_id: "boss-candidate-303",
          name: "钱七",
          age: 28,
          address: "上海",
          job_id: 101,
        })],
      });
      expect(backend.syncPendingCandidateToCandidate).toHaveBeenCalledWith({
        pending_candidate_id: 801,
        run_screening: true,
      });
      expect(backend.updateCrawlTaskPeopleSync).toHaveBeenCalledWith({
        task_id: 303,
        updates: [expect.objectContaining({
          person_id: 2,
          sync_status: "SYNCED",
          candidate_id: 11,
        })],
      });
      expect(backend.runResumeScreening).toHaveBeenCalledWith({
        candidate_id: 11,
        job_id: 101,
      });
    });
  });

  it("marks task failed when cycle still fails after retries", async () => {
    const store = useRecruitingStore();
    await store.bootstrap();

    store.tasks = [
      {
        id: 302,
        source: "boss",
        mode: "compliant",
        task_type: "crawl_candidates",
        status: "PENDING",
        retry_count: 0,
        payload: {
          localJobId: 101,
          localJobTitle: "前端工程师",
          localJobCity: "杭州",
          batchSize: 5,
          crawlIntervalSeconds: 300,
          retryCount: 0,
          retryBackoffMs: 200,
          autoSyncToCandidates: false,
        },
        created_at: "2026-02-26T00:00:00Z",
        updated_at: "2026-02-26T00:00:00Z",
      },
    ];

    backend.listCrawlTasks
      .mockResolvedValueOnce([
        {
          ...store.tasks[0],
          status: "RUNNING",
        },
      ])
      .mockResolvedValueOnce([
        {
          ...store.tasks[0],
          status: "FAILED",
        },
      ]);
    backend.triggerSidecarCrawlJobs.mockResolvedValue({
      id: "sidecar-job-task",
      source: "boss",
      mode: "compliant",
      status: "FAILED",
      attempts: 1,
      error: "job crawl failed",
    });

    await store.toggleTaskRunState(302);

    expect(backend.updateCrawlTask).toHaveBeenCalledWith(expect.objectContaining({
      task_id: 302,
      status: "FAILED",
    }));
  });

  it("deletes running task by pausing first", async () => {
    const store = useRecruitingStore();
    await store.bootstrap();

    store.tasks = [
      {
        id: 401,
        source: "boss",
        mode: "compliant",
        task_type: "crawl_candidates",
        status: "RUNNING",
        retry_count: 0,
        payload: {
          localJobId: 101,
          localJobTitle: "前端工程师",
          localJobCity: "杭州",
          batchSize: 10,
          crawlIntervalSeconds: 300,
          retryCount: 1,
          retryBackoffMs: 450,
          autoSyncToCandidates: true,
        },
        created_at: "2026-02-26T00:00:00Z",
        updated_at: "2026-02-26T00:00:00Z",
      },
    ];

    await store.deleteTask(401);

    expect(backend.updateCrawlTask).toHaveBeenCalledWith(expect.objectContaining({
      task_id: 401,
      status: "PAUSED",
    }));
    expect(backend.deleteCrawlTask).toHaveBeenCalledWith(401);
    expect(store.tasks.some((task) => task.id === 401)).toBe(false);
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

  it("stores hiring decision and keeps ai deviation flag", async () => {
    const store = useRecruitingStore();
    await store.bootstrap();

    const decision = await store.finalizeHiringDecision({
      candidate_id: 11,
      job_id: 101,
      final_decision: "HIRE",
      reason_code: "skills_match",
      note: "业务强相关经验",
    });

    expect(decision.ai_deviation).toBe(true);
    expect(store.hiringDecisions[11][0].final_decision).toBe("HIRE");
    expect(backend.finalizeHiringDecision).toHaveBeenCalledWith(
      expect.objectContaining({
        candidate_id: 11,
        job_id: 101,
        final_decision: "HIRE",
      }),
    );
  });

  it("updates, stops and deletes jobs while syncing local state", async () => {
    const store = useRecruitingStore();
    await store.bootstrap();

    await store.updateJob({
      job_id: 101,
      title: "高级前端工程师",
      company: "示例科技",
      city: "杭州",
      salary_k: "35-50",
      description: "Vue3 + TS + Playwright",
    });
    expect(store.jobs[0]?.title).toBe("高级前端工程师");

    await store.stopJob(101);
    expect(store.jobs[0]?.status).toBe("STOPPED");

    await store.deleteJob(101);
    expect(store.jobs.find((item) => item.id === 101)).toBeUndefined();
    expect(backend.deleteJob).toHaveBeenCalledWith(101);
  });

  it("supports screening template crud and job template binding", async () => {
    const store = useRecruitingStore();
    await store.bootstrap();

    const templates = await store.loadScreeningTemplates();
    expect(templates).toHaveLength(1);

    await store.createScreeningTemplate({
      name: "前端模板",
      dimensions: [
        {
          key: "goal_orientation",
          label: "目标导向",
          weight: 100,
        },
      ],
      risk_rules: {},
    });
    await store.updateScreeningTemplate({
      template_id: 9,
      name: "前端模板v2",
      dimensions: [
        {
          key: "goal_orientation",
          label: "目标导向",
          weight: 100,
        },
      ],
      risk_rules: {},
    });
    await store.deleteScreeningTemplate(9);
    await store.setJobScreeningTemplate({
      job_id: 101,
      template_id: 1,
    });

    expect(backend.listScreeningTemplates).toHaveBeenCalled();
    expect(backend.createScreeningTemplate).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "前端模板",
      }),
    );
    expect(backend.updateScreeningTemplate).toHaveBeenCalledWith(
      expect.objectContaining({
        template_id: 9,
      }),
    );
    expect(backend.deleteScreeningTemplate).toHaveBeenCalledWith(9);
    expect(backend.setJobScreeningTemplate).toHaveBeenCalledWith({
      job_id: 101,
      template_id: 1,
    });
    expect(store.jobs[0]?.screening_template_id).toBe(1);
  });
});
