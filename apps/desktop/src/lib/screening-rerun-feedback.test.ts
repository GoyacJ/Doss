import { describe, expect, it } from "vitest";
import { resolveScreeningRerunFeedback } from "./screening-rerun-feedback";

describe("resolveScreeningRerunFeedback", () => {
  it("maps missing resume error to warning with localized message", () => {
    expect(resolveScreeningRerunFeedback(new Error("Resume required before screening"))).toEqual({
      tone: "warning",
      message: "请先上传简历后再重新分析",
    });
  });

  it("maps missing resume for analysis to warning with localized message", () => {
    expect(resolveScreeningRerunFeedback(new Error("Resume required before analysis"))).toEqual({
      tone: "warning",
      message: "请先上传简历后再重新分析",
    });
  });

  it("keeps unknown error as danger message", () => {
    expect(resolveScreeningRerunFeedback(new Error("network_timeout"))).toEqual({
      tone: "danger",
      message: "network_timeout",
    });
  });
});
