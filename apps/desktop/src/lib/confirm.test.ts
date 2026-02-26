import { describe, expect, it } from "vitest";
import { safeConfirm } from "./confirm";

describe("safeConfirm", () => {
  it("returns fallback true when confirm api is unavailable", () => {
    const result = safeConfirm("删除确认", {
      confirmFn: undefined,
      fallbackWhenUnavailable: true,
    });
    expect(result).toBe(true);
  });

  it("calls native confirm when provided", () => {
    const result = safeConfirm("删除确认", {
      confirmFn: () => false,
    });
    expect(result).toBe(false);
  });
});

