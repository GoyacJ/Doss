import { describe, expect, it } from "vitest";
import {
  buildScorecard,
  isValidPipelineTransition,
  type CandidateProfile,
  type JobContext,
} from "../src";

describe("pipeline transition rules", () => {
  it("allows NEW -> SCREENING", () => {
    expect(isValidPipelineTransition("NEW", "SCREENING")).toBe(true);
  });

  it("rejects NEW -> OFFERED", () => {
    expect(isValidPipelineTransition("NEW", "OFFERED")).toBe(false);
  });
});

describe("AI scorecard generation", () => {
  it("returns explainable scorecard with stable dimensions", () => {
    const profile: CandidateProfile = {
      basic: { name: "Alice", yearsOfExperience: 6, expectedSalaryK: 45 },
      skills: ["Vue3", "TypeScript", "Playwright", "SQL"],
      experiences: [
        { company: "A", years: 2.5, title: "Frontend Engineer" },
        { company: "B", years: 3.5, title: "Fullstack Engineer" },
      ],
      education: [{ school: "Example University", degree: "CS" }],
      projects: [{ name: "Recruiting Dashboard", impact: "Reduced screening time 30%" }],
      rawText: "Alice has 6 years experience and strong Vue + TypeScript skills.",
    };

    const job: JobContext = {
      title: "Senior Frontend Engineer",
      mustHaveSkills: ["Vue3", "TypeScript"],
      niceToHaveSkills: ["Playwright", "Tauri"],
      maxSalaryK: 50,
      minYears: 5,
    };

    const result = buildScorecard(profile, job);

    expect(result.overallScore).toBeGreaterThanOrEqual(70);
    expect(result.dimensionScores).toHaveLength(4);
    expect(result.evidence.length).toBeGreaterThan(0);
    expect(result.modelInfo.provider).toBe("local-heuristic");
  });
});
