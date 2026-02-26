import { describe, expect, it } from "vitest";
import { formatStageLabel, nextStageOptions } from "./pipeline";

describe("pipeline helpers", () => {
  it("formats stage label for UI", () => {
    expect(formatStageLabel("SCREENING")).toBe("初筛");
  });

  it("returns valid next stage options", () => {
    expect(nextStageOptions("NEW")).toEqual(["SCREENING", "HOLD", "REJECTED"]);
  });
});
