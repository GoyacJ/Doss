import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

import {
  checkSidecarHealth,
  ensureSidecar,
  createScreeningTemplate,
  createCandidate,
  createCrawlTask,
  createJob,
  deleteCandidate,
  deleteCrawlTask,
  deleteAiProviderProfile,
  deleteJob,
  deleteScreeningTemplate,
  finalizeHiringDecision,
  listCandidatesPage,
  listDecisionCandidatesPage,
  listInterviewCandidatesPage,
  listPendingCandidates,
  listAnalysis,
  listHiringDecisions,
  listInterviewEvaluations,
  listPipelineEvents,
  listScreeningTemplates,
  listScreeningResults,
  parseResumeFile,
  generateInterviewKit,
  getScreeningTemplate,
  runCandidateAnalysis,
  runInterviewEvaluation,
  runResumeScreening,
  saveInterviewRecording,
  saveInterviewKit,
  setCandidateQualification,
  setDefaultAiProviderProfile,
  syncPendingCandidateToCandidate,
  submitInterviewFeedback,
  testAiProviderProfile,
  triggerSidecarCrawlCandidates,
  triggerSidecarCrawlJobs,
  triggerSidecarCrawlResume,
  setJobScreeningTemplate,
  stopJob,
  listCrawlTaskPeople,
  updateCandidate,
  updateJob,
  updateScreeningTemplate,
  updateCrawlTask,
  updateCrawlTaskPeopleSync,
  upsertResume,
  upsertCrawlTaskPeople,
  upsertPendingCandidates,
  upsertTaskRuntimeSettings,
} from "./backend";

describe("backend AI profile commands", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue([]);
  });

  it("passes camelCase arg for delete profile command", async () => {
    await deleteAiProviderProfile("profile-1");

    expect(invokeMock).toHaveBeenCalledWith("delete_ai_provider_profile", {
      profileId: "profile-1",
    });
  });

  it("passes camelCase arg for set default profile command", async () => {
    await setDefaultAiProviderProfile("profile-2");

    expect(invokeMock).toHaveBeenCalledWith("set_default_ai_provider_profile", {
      profileId: "profile-2",
    });
  });

  it("passes camelCase arg for test profile command", async () => {
    await testAiProviderProfile("profile-3");

    expect(invokeMock).toHaveBeenCalledWith("test_ai_provider_profile", {
      profileId: "profile-3",
    });
  });

  it("routes sidecar health and crawl calls via tauri invoke", async () => {
    invokeMock
      .mockResolvedValueOnce({
        ok: true,
        port: 3791,
        base_url: "http://127.0.0.1:3791",
        source: "existing",
        restart_count: 0,
      })
      .mockResolvedValueOnce({
        ok: true,
        service: "crawler-sidecar",
      })
      .mockResolvedValueOnce({
        id: "jobs-1",
        source: "boss",
        mode: "compliant",
        status: "SUCCEEDED",
        attempts: 1,
      })
      .mockResolvedValueOnce({
        id: "candidates-1",
        source: "boss",
        mode: "compliant",
        status: "SUCCEEDED",
        attempts: 1,
      })
      .mockResolvedValueOnce({
        id: "resume-1",
        source: "boss",
        mode: "compliant",
        status: "SUCCEEDED",
        attempts: 1,
      });

    await ensureSidecar();
    await checkSidecarHealth();
    await triggerSidecarCrawlJobs({
      source: "boss",
      mode: "compliant",
      keyword: "前端",
      city: "上海",
    });
    await triggerSidecarCrawlCandidates({
      source: "boss",
      mode: "compliant",
      jobId: "job-101",
    });
    await triggerSidecarCrawlResume({
      source: "boss",
      mode: "compliant",
      candidateId: "candidate-7",
    });

    expect(invokeMock).toHaveBeenNthCalledWith(1, "ensure_sidecar");
    expect(invokeMock).toHaveBeenNthCalledWith(2, "sidecar_health");
    expect(invokeMock).toHaveBeenNthCalledWith(3, "sidecar_crawl_jobs", {
      input: {
        source: "boss",
        mode: "compliant",
        keyword: "前端",
        city: "上海",
      },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(4, "sidecar_crawl_candidates", {
      input: {
        source: "boss",
        mode: "compliant",
        job_id: "job-101",
      },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(5, "sidecar_crawl_resume", {
      input: {
        source: "boss",
        mode: "compliant",
        candidate_id: "candidate-7",
      },
    });
  });

  it("passes snake_case args for screening commands", async () => {
    await getScreeningTemplate(12);
    await runResumeScreening({
      candidate_id: 101,
      job_id: 12,
    });

    expect(invokeMock).toHaveBeenCalledWith("get_screening_template", {
      job_id: 12,
    });
    expect(invokeMock).toHaveBeenCalledWith("run_resume_screening", {
      input: {
        candidate_id: 101,
        job_id: 12,
      },
    });
  });

  it("passes camelCase args for candidate timeline and analysis commands", async () => {
    await listPipelineEvents(101);
    await listAnalysis(102);
    await listScreeningResults(103);
    await listHiringDecisions(104);
    await listInterviewEvaluations(105);
    await runCandidateAnalysis({
      candidate_id: 106,
      job_id: 12,
    });

    expect(invokeMock).toHaveBeenCalledWith("list_pipeline_events", {
      candidateId: 101,
    });
    expect(invokeMock).toHaveBeenCalledWith("list_analysis", {
      candidateId: 102,
    });
    expect(invokeMock).toHaveBeenCalledWith("list_screening_results", {
      candidateId: 103,
    });
    expect(invokeMock).toHaveBeenCalledWith("list_hiring_decisions", {
      candidateId: 104,
    });
    expect(invokeMock).toHaveBeenCalledWith("list_interview_evaluations", {
      candidateId: 105,
    });
    expect(invokeMock).toHaveBeenCalledWith("run_candidate_analysis", {
      input: {
        candidate_id: 106,
        job_id: 12,
      },
    });
  });

  it("passes paged list queries for candidates/interview/decision commands", async () => {
    await listCandidatesPage({
      page: 2,
      page_size: 20,
      name_like: "张",
      sorts: [
        {
          field: "score",
          direction: "desc",
        },
      ],
    });
    await listInterviewCandidatesPage({
      page: 1,
      page_size: 10,
      job_id: 12,
      sorts: [
        {
          field: "updated_at",
          direction: "desc",
        },
      ],
    });
    await listDecisionCandidatesPage({
      page: 3,
      page_size: 10,
      interview_passed: true,
      sorts: [
        {
          field: "updated_at",
          direction: "desc",
        },
      ],
    });

    expect(invokeMock).toHaveBeenCalledWith("list_candidates_page", {
      input: {
        page: 2,
        page_size: 20,
        name_like: "张",
        sorts: [
          {
            field: "score",
            direction: "desc",
          },
        ],
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("list_interview_candidates_page", {
      input: {
        page: 1,
        page_size: 10,
        job_id: 12,
        sorts: [
          {
            field: "updated_at",
            direction: "desc",
          },
        ],
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("list_decision_candidates_page", {
      input: {
        page: 3,
        page_size: 10,
        interview_passed: true,
        sorts: [
          {
            field: "updated_at",
            direction: "desc",
          },
        ],
      },
    });
  });

  it("passes nested input payloads for candidate and crawl mutation commands", async () => {
    await createCandidate({
      source: "manual",
      external_id: "ext-1",
      name: "张三",
      current_company: "Doss",
      score: 88,
      age: 28,
      gender: "male",
      years_of_experience: 5,
      address: "上海市浦东新区",
      phone: "13800000000",
      email: "zhangsan@example.com",
      tags: ["vue3"],
      job_id: 9,
    });
    await upsertResume({
      candidate_id: 1,
      source: "manual",
      raw_text: "resume text",
      parsed: {
        skills: ["vue3"],
      },
    });
    await parseResumeFile({
      file_name: "resume.pdf",
      content_base64: "aGVsbG8=",
      enable_ocr: true,
    });
    await createCrawlTask({
      source: "boss",
      mode: "compliant",
      task_type: "candidate",
      payload: {
        keyword: "frontend",
      },
      schedule_type: "DAILY",
      schedule_time: "09:30",
    });
    await updateCrawlTask({
      task_id: 11,
      status: "RUNNING",
      retry_count: 2,
      error_code: "none",
      snapshot: {
        progress: 50,
      },
      schedule_type: "MONTHLY",
      schedule_time: "10:00",
      schedule_day: 15,
      next_run_at: "2026-03-15T02:00:00.000Z",
    });
    await upsertTaskRuntimeSettings({
      auto_batch_concurrency: 4,
      auto_retry_count: 2,
      auto_retry_backoff_ms: 500,
    });

    expect(invokeMock).toHaveBeenCalledWith("create_candidate", {
      input: {
        source: "manual",
        external_id: "ext-1",
        name: "张三",
        current_company: "Doss",
        score: 88,
        age: 28,
        gender: "male",
        years_of_experience: 5,
        address: "上海市浦东新区",
        phone: "13800000000",
        email: "zhangsan@example.com",
        tags: ["vue3"],
        job_id: 9,
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("upsert_resume", {
      input: {
        candidate_id: 1,
        source: "manual",
        raw_text: "resume text",
        parsed: {
          skills: ["vue3"],
        },
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("parse_resume_file", {
      input: {
        file_name: "resume.pdf",
        content_base64: "aGVsbG8=",
        enable_ocr: true,
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("create_crawl_task", {
      input: {
        source: "boss",
        mode: "compliant",
        task_type: "candidate",
        payload: {
          keyword: "frontend",
        },
        schedule_type: "DAILY",
        schedule_time: "09:30",
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("update_crawl_task", {
      input: {
        task_id: 11,
        status: "RUNNING",
        retry_count: 2,
        error_code: "none",
        snapshot: {
          progress: 50,
        },
        schedule_type: "MONTHLY",
        schedule_time: "10:00",
        schedule_day: 15,
        next_run_at: "2026-03-15T02:00:00.000Z",
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("upsert_task_runtime_settings", {
      input: {
        auto_batch_concurrency: 4,
        auto_retry_count: 2,
        auto_retry_backoff_ms: 500,
      },
    });
  });

  it("passes nested input payloads for candidate edit/delete/qualification commands", async () => {
    await updateCandidate({
      candidate_id: 18,
      name: "李四",
      current_company: "Doss Labs",
      score: 92,
      age: 31,
      gender: "female",
      years_of_experience: 6.5,
      phone: "13900000000",
      email: "lisi@example.com",
      tags: ["vue3", "candidate"],
      job_id: 9,
    });
    await setCandidateQualification({
      candidate_id: 18,
      qualified: false,
      note: "背景核验未通过",
    });
    await deleteCandidate(18);

    expect(invokeMock).toHaveBeenCalledWith("update_candidate", {
      input: {
        candidate_id: 18,
        name: "李四",
        current_company: "Doss Labs",
        score: 92,
        age: 31,
        gender: "female",
        years_of_experience: 6.5,
        phone: "13900000000",
        email: "lisi@example.com",
        tags: ["vue3", "candidate"],
        job_id: 9,
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("set_candidate_qualification", {
      input: {
        candidate_id: 18,
        qualified: false,
        note: "背景核验未通过",
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("delete_candidate", {
      candidateId: 18,
    });
  });

  it("passes payloads for job CRUD and stop commands", async () => {
    await createJob({
      source: "manual",
      title: "前端工程师",
      company: "示例科技",
      city: "上海",
      salary_k: "30-45",
      description: "Vue3 + TS",
    });
    await updateJob({
      job_id: 12,
      title: "高级前端工程师",
      company: "示例科技",
      city: "杭州",
      salary_k: "35-50",
      description: "Vue3 + TS + Playwright",
    });
    await stopJob(12);
    await deleteJob(12);

    expect(invokeMock).toHaveBeenCalledWith("create_job", {
      input: {
        source: "manual",
        title: "前端工程师",
        company: "示例科技",
        city: "上海",
        salary_k: "30-45",
        description: "Vue3 + TS",
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("update_job", {
      input: {
        job_id: 12,
        title: "高级前端工程师",
        company: "示例科技",
        city: "杭州",
        salary_k: "35-50",
        description: "Vue3 + TS + Playwright",
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("stop_job", {
      job_id: 12,
    });
    expect(invokeMock).toHaveBeenCalledWith("delete_job", {
      jobId: 12,
    });
  });

  it("passes payloads for screening template list/crud and job binding commands", async () => {
    await listScreeningTemplates();
    await createScreeningTemplate({
      name: "前端筛选模板",
      dimensions: [
        {
          key: "goal_orientation",
          label: "目标导向",
          weight: 100,
        },
      ],
      risk_rules: {
        highRiskKeywords: ["频繁跳槽"],
      },
    });
    await updateScreeningTemplate({
      template_id: 6,
      name: "前端筛选模板 v2",
      dimensions: [
        {
          key: "team_collaboration",
          label: "团队协作",
          weight: 100,
        },
      ],
      risk_rules: {},
    });
    await setJobScreeningTemplate({
      job_id: 12,
      template_id: 6,
    });
    await deleteScreeningTemplate(6);

    expect(invokeMock).toHaveBeenCalledWith("list_screening_templates");
    expect(invokeMock).toHaveBeenCalledWith("create_screening_template", {
      input: {
        name: "前端筛选模板",
        dimensions: [
          {
            key: "goal_orientation",
            label: "目标导向",
            weight: 100,
          },
        ],
        risk_rules: {
          highRiskKeywords: ["频繁跳槽"],
        },
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("update_screening_template", {
      input: {
        template_id: 6,
        name: "前端筛选模板 v2",
        dimensions: [
          {
            key: "team_collaboration",
            label: "团队协作",
            weight: 100,
          },
        ],
        risk_rules: {},
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("set_job_screening_template", {
      input: {
        job_id: 12,
        template_id: 6,
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("delete_screening_template", {
      templateId: 6,
    });
  });

  it("passes payloads for crawl task people commands", async () => {
    await deleteCrawlTask(21);
    await listCrawlTaskPeople(21);
    await upsertCrawlTaskPeople({
      task_id: 21,
      people: [
        {
          source: "boss",
          external_id: "boss-candidate-21",
          name: "王五",
          current_company: "示例科技",
          years_of_experience: 4,
          sync_status: "UNSYNCED",
        },
      ],
    });
    await updateCrawlTaskPeopleSync({
      task_id: 21,
      updates: [
        {
          person_id: 2101,
          sync_status: "SYNCED",
          candidate_id: 88,
        },
      ],
    });

    expect(invokeMock).toHaveBeenCalledWith("delete_crawl_task", {
      taskId: 21,
    });
    expect(invokeMock).toHaveBeenCalledWith("list_crawl_task_people", {
      taskId: 21,
    });
    expect(invokeMock).toHaveBeenCalledWith("upsert_crawl_task_people", {
      input: {
        task_id: 21,
        people: [
          {
            source: "boss",
            external_id: "boss-candidate-21",
            name: "王五",
            current_company: "示例科技",
            years_of_experience: 4,
            sync_status: "UNSYNCED",
          },
        ],
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("update_crawl_task_people_sync", {
      input: {
        task_id: 21,
        updates: [
          {
            person_id: 2101,
            sync_status: "SYNCED",
            candidate_id: 88,
          },
        ],
      },
    });
  });

  it("passes snake_case args for interview commands", async () => {
    await generateInterviewKit({
      candidate_id: 101,
      job_id: 12,
    });
    await saveInterviewKit({
      candidate_id: 101,
      job_id: 12,
      questions: [],
    });
    await submitInterviewFeedback({
      candidate_id: 101,
      job_id: 12,
      transcript_text: "面试转写",
      structured_feedback: {
        scores: {
          communication: 4,
        },
      },
    });
    await runInterviewEvaluation({
      candidate_id: 101,
      job_id: 12,
    });
    await saveInterviewRecording({
      file_name: "candidate-101.m4a",
      content_base64: "aGVsbG8=",
    });

    expect(invokeMock).toHaveBeenCalledWith("generate_interview_kit", {
      input: {
        candidate_id: 101,
        job_id: 12,
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("save_interview_kit", {
      input: {
        candidate_id: 101,
        job_id: 12,
        questions: [],
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("submit_interview_feedback", {
      input: {
        candidate_id: 101,
        job_id: 12,
        transcript_text: "面试转写",
        structured_feedback: {
          scores: {
            communication: 4,
          },
        },
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("run_interview_evaluation", {
      input: {
        candidate_id: 101,
        job_id: 12,
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("save_interview_recording", {
      input: {
        file_name: "candidate-101.m4a",
        content_base64: "aGVsbG8=",
      },
    });
  });

  it("passes pending candidate commands with nested input payload", async () => {
    await upsertPendingCandidates({
      items: [
        {
          source: "boss",
          external_id: "boss-pending-1",
          name: "王五",
          current_company: "示例科技",
          years_of_experience: 6,
          age: 29,
          address: "上海",
          tags: ["source:boss"],
        },
      ],
    });
    await listPendingCandidates({
      page: 1,
      page_size: 20,
      sync_status: "UNSYNCED",
      name_like: "王",
    });
    await syncPendingCandidateToCandidate({
      pending_candidate_id: 18,
      run_screening: true,
    });

    expect(invokeMock).toHaveBeenCalledWith("upsert_pending_candidates", {
      input: {
        items: [
          {
            source: "boss",
            external_id: "boss-pending-1",
            name: "王五",
            current_company: "示例科技",
            years_of_experience: 6,
            age: 29,
            address: "上海",
            tags: ["source:boss"],
          },
        ],
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("list_pending_candidates", {
      input: {
        page: 1,
        page_size: 20,
        sync_status: "UNSYNCED",
        name_like: "王",
      },
    });
    expect(invokeMock).toHaveBeenCalledWith("sync_pending_candidate_to_candidate", {
      input: {
        pending_candidate_id: 18,
        run_screening: true,
      },
    });
  });

  it("passes snake_case args for hiring decision command", async () => {
    await finalizeHiringDecision({
      candidate_id: 101,
      job_id: 12,
      final_decision: "HIRE",
      reason_code: "skills_match",
      note: "核心能力匹配度高",
    });

    expect(invokeMock).toHaveBeenCalledWith("finalize_hiring_decision", {
      input: {
        candidate_id: 101,
        job_id: 12,
        final_decision: "HIRE",
        reason_code: "skills_match",
        note: "核心能力匹配度高",
      },
    });
  });
});
