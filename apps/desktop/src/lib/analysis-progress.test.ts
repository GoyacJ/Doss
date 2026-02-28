import { describe, expect, it } from "vitest";
import {
  appendAnalysisTrace,
  buildFallbackAnalysisMessage,
  resolveAnalysisStepIndex,
  shouldAcceptAnalysisProgressEvent,
} from "./analysis-progress";

describe("analysis progress helpers", () => {
  it("filters events by runId and candidateId", () => {
    const payload = {
      runId: "run-1",
      candidateId: 7,
      phase: "prepare" as const,
      status: "running" as const,
      kind: "start" as const,
      message: "start",
      at: "2026-02-28T10:00:00.000Z",
    };

    expect(shouldAcceptAnalysisProgressEvent(payload, "run-1", 7)).toBe(true);
    expect(shouldAcceptAnalysisProgressEvent(payload, "run-2", 7)).toBe(false);
    expect(shouldAcceptAnalysisProgressEvent(payload, "run-1", 8)).toBe(false);
  });

  it("advances step index forward and never regresses", () => {
    let index = 0;
    index = resolveAnalysisStepIndex(index, "prepare", "running");
    expect(index).toBe(0);

    index = resolveAnalysisStepIndex(index, "ai", "running");
    expect(index).toBe(1);

    index = resolveAnalysisStepIndex(index, "prepare", "running");
    expect(index).toBe(1);

    index = resolveAnalysisStepIndex(index, "persist", "completed");
    expect(index).toBe(2);
  });

  it("sorts traces by time and trims to max items", () => {
    const trace = appendAnalysisTrace(
      [],
      {
        runId: "run-1",
        candidateId: 7,
        phase: "ai",
        status: "running",
        kind: "progress",
        message: "b",
        at: "2026-02-28T10:00:02.000Z",
      },
      2,
    );
    const merged = appendAnalysisTrace(
      trace,
      {
        runId: "run-1",
        candidateId: 7,
        phase: "prepare",
        status: "running",
        kind: "start",
        message: "a",
        at: "2026-02-28T10:00:01.000Z",
      },
      2,
    );
    const trimmed = appendAnalysisTrace(
      merged,
      {
        runId: "run-1",
        candidateId: 7,
        phase: "persist",
        status: "running",
        kind: "progress",
        message: "c",
        at: "2026-02-28T10:00:03.000Z",
      },
      2,
    );

    expect(trimmed).toHaveLength(2);
    expect(trimmed[0].message).toBe("b");
    expect(trimmed[1].message).toBe("c");
  });

  it("returns fallback message for each phase", () => {
    expect(buildFallbackAnalysisMessage("prepare")).toContain("上下文");
    expect(buildFallbackAnalysisMessage("ai")).toContain("评估");
    expect(buildFallbackAnalysisMessage("persist")).toContain("刷新");
  });
});

