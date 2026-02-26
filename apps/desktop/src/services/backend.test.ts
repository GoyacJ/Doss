import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

import {
  createCandidate,
  createCrawlTask,
  deleteAiProviderProfile,
  finalizeHiringDecision,
  listAnalysis,
  listHiringDecisions,
  listInterviewEvaluations,
  listPipelineEvents,
  listScreeningResults,
  parseResumeFile,
  generateInterviewKit,
  getScreeningTemplate,
  runCandidateAnalysis,
  runInterviewEvaluation,
  runResumeScreening,
  saveInterviewKit,
  setDefaultAiProviderProfile,
  submitInterviewFeedback,
  testAiProviderProfile,
  updateCrawlTask,
  upsertResume,
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

  it("passes snake_case args for candidate timeline and analysis commands", async () => {
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
      candidate_id: 101,
    });
    expect(invokeMock).toHaveBeenCalledWith("list_analysis", {
      candidate_id: 102,
    });
    expect(invokeMock).toHaveBeenCalledWith("list_screening_results", {
      candidate_id: 103,
    });
    expect(invokeMock).toHaveBeenCalledWith("list_hiring_decisions", {
      candidate_id: 104,
    });
    expect(invokeMock).toHaveBeenCalledWith("list_interview_evaluations", {
      candidate_id: 105,
    });
    expect(invokeMock).toHaveBeenCalledWith("run_candidate_analysis", {
      input: {
        candidate_id: 106,
        job_id: 12,
      },
    });
  });

  it("passes nested input payloads for candidate and crawl mutation commands", async () => {
    await createCandidate({
      source: "manual",
      external_id: "ext-1",
      name: "张三",
      current_company: "Doss",
      years_of_experience: 5,
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
      source: "manual",
      mode: "compliant",
      task_type: "candidate",
      payload: {
        keyword: "frontend",
      },
    });
    await updateCrawlTask({
      task_id: 11,
      status: "RUNNING",
      retry_count: 2,
      error_code: "none",
      snapshot: {
        progress: 50,
      },
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
        years_of_experience: 5,
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
        source: "manual",
        mode: "compliant",
        task_type: "candidate",
        payload: {
          keyword: "frontend",
        },
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
