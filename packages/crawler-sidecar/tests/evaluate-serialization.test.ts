import path from "node:path";
import { execFileSync } from "node:child_process";
import { describe, expect, it } from "vitest";

function runProbe() {
  const testDir = path.dirname(new URL(import.meta.url).pathname);
  const packageRoot = path.resolve(testDir, "..");
  const probePath = path.join(packageRoot, "tests/fixtures/evaluate-probe.ts");
  const stdout = execFileSync("pnpm", ["exec", "tsx", probePath], {
    cwd: packageRoot,
    encoding: "utf8",
  });

  return JSON.parse(stdout) as {
    jobHasHelper: boolean;
    candidateHasHelper: boolean;
  };
}

describe("playwright evaluate callback serialization", () => {
  it("does not leak bundler helpers in tsx runtime", () => {
    const output = runProbe();

    expect(output.jobHasHelper).toBe(false);
    expect(output.candidateHasHelper).toBe(false);
  });
});
