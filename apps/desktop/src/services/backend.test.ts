import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

import {
  deleteAiProviderProfile,
  generateInterviewKit,
  getScreeningTemplate,
  runInterviewEvaluation,
  runResumeScreening,
  saveInterviewKit,
  setDefaultAiProviderProfile,
  submitInterviewFeedback,
  testAiProviderProfile,
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
});
