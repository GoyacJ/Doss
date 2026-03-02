import { describe, expect, it } from "vitest";
import { resolveScoringRerunFeedback } from "./scoring-rerun-feedback";

describe("resolveScoringRerunFeedback", () => {
  it("maps resume file required errors to localized warning message", () => {
    expect(resolveScoringRerunFeedback(new Error("resume_file_required_for_ai_analysis"))).toEqual({
      tone: "warning",
      message: "请先上传简历文件后再重新分析",
    });
  });

  it("maps empty parsed resume text errors to localized warning message", () => {
    expect(resolveScoringRerunFeedback(new Error("resume_file_text_empty_after_parse"))).toEqual({
      tone: "warning",
      message: "简历文件解析为空，请检查文件内容或OCR设置",
    });
  });

  it("maps context overflow errors to localized danger message", () => {
    expect(resolveScoringRerunFeedback(new Error("provider_http_400: context length exceeded"))).toEqual({
      tone: "danger",
      message: "简历全文超过当前模型上下文上限，请切换长上下文模型后重试",
    });
  });

  it("maps provider_response_not_json_after_repair to localized danger message", () => {
    expect(resolveScoringRerunFeedback(new Error("provider_response_not_json_after_repair"))).toEqual({
      tone: "danger",
      message: "模型返回格式异常，请稍后重试",
    });
  });

  it("maps provider_response_schema_invalid to localized danger message", () => {
    expect(resolveScoringRerunFeedback(new Error("provider_response_schema_invalid"))).toEqual({
      tone: "danger",
      message: "模型结果字段不完整，请稍后重试",
    });
  });

  it("maps scoring task join errors to timeout/interrupted message", () => {
    expect(resolveScoringRerunFeedback(new Error("scoring_task_join_error: join error"))).toEqual({
      tone: "danger",
      message: "评分任务超时或中断，请稍后重试",
    });
  });
});
