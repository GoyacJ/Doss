import { describe, expect, it } from "vitest";
import {
  interviewRecommendationLabel,
  interviewRecommendationTone,
  jobStatusLabel,
  jobStatusTone,
  sidecarHealthBadge,
  stageTone,
  taskStatusLabel,
  taskStatusTone,
} from "./status";

describe("status helpers", () => {
  it("maps pipeline stage to badge tone", () => {
    expect(stageTone("NEW")).toBe("neutral");
    expect(stageTone("SCREENING")).toBe("info");
    expect(stageTone("INTERVIEW")).toBe("info");
    expect(stageTone("HOLD")).toBe("warning");
    expect(stageTone("REJECTED")).toBe("danger");
    expect(stageTone("OFFERED")).toBe("success");
  });

  it("maps crawl task status to localized label", () => {
    expect(taskStatusLabel("PENDING")).toBe("待执行");
    expect(taskStatusLabel("RUNNING")).toBe("进行中");
    expect(taskStatusLabel("PAUSED")).toBe("已暂停");
    expect(taskStatusLabel("CANCELED")).toBe("已取消");
    expect(taskStatusLabel("SUCCEEDED")).toBe("成功");
    expect(taskStatusLabel("FAILED")).toBe("失败");
  });

  it("maps crawl task status to badge tone", () => {
    expect(taskStatusTone("PENDING")).toBe("neutral");
    expect(taskStatusTone("RUNNING")).toBe("info");
    expect(taskStatusTone("PAUSED")).toBe("warning");
    expect(taskStatusTone("CANCELED")).toBe("warning");
    expect(taskStatusTone("SUCCEEDED")).toBe("success");
    expect(taskStatusTone("FAILED")).toBe("danger");
  });

  it("maps sidecar health to badge payload", () => {
    expect(sidecarHealthBadge(true)).toEqual({
      label: "在线",
      tone: "success",
    });
    expect(sidecarHealthBadge(false)).toEqual({
      label: "离线",
      tone: "danger",
    });
    expect(sidecarHealthBadge(null)).toEqual({
      label: "未知",
      tone: "neutral",
    });
  });

  it("maps interview recommendation to label and tone", () => {
    expect(interviewRecommendationLabel("HIRE")).toBe("建议录用");
    expect(interviewRecommendationLabel("HOLD")).toBe("建议待定");
    expect(interviewRecommendationLabel("NO_HIRE")).toBe("建议不录用");

    expect(interviewRecommendationTone("HIRE")).toBe("success");
    expect(interviewRecommendationTone("HOLD")).toBe("warning");
    expect(interviewRecommendationTone("NO_HIRE")).toBe("danger");
  });

  it("maps job status to label and tone", () => {
    expect(jobStatusLabel("ACTIVE")).toBe("招聘中");
    expect(jobStatusLabel("STOPPED")).toBe("已停止");
    expect(jobStatusLabel()).toBe("招聘中");

    expect(jobStatusTone("ACTIVE")).toBe("success");
    expect(jobStatusTone("STOPPED")).toBe("warning");
    expect(jobStatusTone()).toBe("success");
  });
});
