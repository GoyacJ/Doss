export type SourceType = "boss" | "zhilian" | "wuba" | "lagou" | "all" | "manual";
export type CrawlTaskSource = "boss" | "zhilian" | "wuba" | "lagou" | "all";
export type CrawlPlatformSource = "boss" | "zhilian" | "wuba" | "lagou";

export type CrawlMode = "compliant" | "advanced";

export type CrawlTaskStatus =
  | "PENDING"
  | "RUNNING"
  | "PAUSED"
  | "CANCELED"
  | "SUCCEEDED"
  | "FAILED";

export type JobStatus = "ACTIVE" | "STOPPED";

export type PipelineStage =
  | "NEW"
  | "SCREENING"
  | "INTERVIEW"
  | "HOLD"
  | "REJECTED"
  | "OFFERED";

export type CandidateGender = "male" | "female" | "other";
export type CrawlTaskScheduleType = "ONCE" | "DAILY" | "MONTHLY";

export interface PageQuery {
  page?: number;
  page_size?: number;
}

export interface PageResult<T> {
  items: T[];
  page: number;
  page_size: number;
  total: number;
}

export interface CandidateBasic {
  name: string;
  yearsOfExperience: number;
  expectedSalaryK?: number;
}

export interface CandidateExperience {
  company: string;
  years: number;
  title: string;
}

export interface CandidateEducation {
  school: string;
  degree: string;
}

export interface CandidateProject {
  name: string;
  impact: string;
}

export interface CandidateProfile {
  basic: CandidateBasic;
  skills: string[];
  experiences: CandidateExperience[];
  education: CandidateEducation[];
  projects: CandidateProject[];
  rawText: string;
}

export interface JobContext {
  title: string;
  mustHaveSkills: string[];
  niceToHaveSkills: string[];
  minYears: number;
  maxSalaryK?: number;
}

export interface DimensionScore {
  key: "skill_match" | "experience" | "compensation" | "stability";
  score: number;
  reason: string;
}

export interface EvidenceItem {
  dimension: string;
  statement: string;
  sourceSnippet: string;
}

export interface AnalysisResult {
  overallScore: number;
  dimensionScores: DimensionScore[];
  risks: string[];
  highlights: string[];
  suggestions: string[];
  evidence: EvidenceItem[];
  modelInfo: {
    provider: string;
    model: string;
    generatedAt: string;
  };
}

export type ScreeningRecommendation = "PASS" | "REVIEW" | "REJECT";
export type ScreeningRiskLevel = "LOW" | "MEDIUM" | "HIGH";

export interface ScreeningDimension {
  key: string;
  label: string;
  weight: number;
}

export interface ScreeningTemplate {
  id: number;
  scope: "global" | "job";
  name: string;
  job_id?: number | null;
  dimensions: ScreeningDimension[];
  risk_rules: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface ScreeningResult {
  id: number;
  candidate_id: number;
  job_id?: number | null;
  template_id?: number | null;
  t0_score: number;
  t1_score: number;
  fine_score: number;
  bonus_score: number;
  risk_penalty: number;
  overall_score: number;
  recommendation: ScreeningRecommendation;
  risk_level: ScreeningRiskLevel;
  evidence: string[];
  verification_points: string[];
  structured_result: Record<string, unknown>;
  created_at: string;
}

export type InterviewRecommendation = "HIRE" | "HOLD" | "NO_HIRE";

export interface InterviewQuestion {
  primary_question: string;
  follow_ups: string[];
  scoring_points: string[];
  red_flags: string[];
}

export interface InterviewKit {
  id?: number;
  candidate_id: number;
  job_id?: number;
  questions: InterviewQuestion[];
  generated_by: string;
  created_at: string;
  updated_at: string;
}

export interface InterviewFeedback {
  id: number;
  candidate_id: number;
  job_id?: number;
  transcript_text: string;
  structured_feedback: Record<string, unknown>;
  recording_path?: string;
  created_at: string;
  updated_at: string;
}

export interface InterviewEvaluation {
  id: number;
  candidate_id: number;
  job_id?: number;
  feedback_id: number;
  recommendation: InterviewRecommendation;
  overall_score: number;
  confidence: number;
  evidence: string[];
  verification_points: string[];
  uncertainty: string;
  created_at: string;
}

export type HiringFinalDecision = "HIRE" | "NO_HIRE";

export interface HiringDecision {
  id: number;
  candidate_id: number;
  job_id?: number | null;
  interview_evaluation_id?: number | null;
  ai_recommendation?: InterviewRecommendation | null;
  final_decision: HiringFinalDecision;
  reason_code: string;
  note?: string | null;
  ai_deviation: boolean;
  created_at: string;
  updated_at: string;
}

export interface JobRecord {
  id: number;
  external_id?: string | null;
  source: string;
  title: string;
  company: string;
  city?: string | null;
  salary_k?: string | null;
  description?: string | null;
  status?: JobStatus;
  screening_template_id?: number | null;
  screening_template_name?: string | null;
  created_at: string;
  updated_at: string;
}

export interface CandidateRecord {
  id: number;
  external_id?: string | null;
  source: string;
  name: string;
  current_company?: string | null;
  job_id?: number | null;
  job_title?: string | null;
  score?: number | null;
  age?: number | null;
  gender?: CandidateGender | null;
  years_of_experience: number;
  address?: string | null;
  stage: PipelineStage;
  tags: string[];
  phone_masked?: string | null;
  email_masked?: string | null;
  created_at: string;
  updated_at: string;
}

export interface ResumeRecord {
  id: number;
  candidate_id: number;
  source: string;
  raw_text: string;
  parsed: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface CrawlTaskRecord {
  id: number;
  source: string;
  mode: CrawlMode;
  task_type: string;
  status: CrawlTaskStatus;
  retry_count: number;
  error_code?: string | null;
  payload: Record<string, unknown>;
  snapshot?: Record<string, unknown> | null;
  schedule_type?: CrawlTaskScheduleType;
  schedule_time?: string | null;
  schedule_day?: number | null;
  next_run_at?: string | null;
  started_at?: string | null;
  finished_at?: string | null;
  created_at: string;
  updated_at: string;
}

export type SortDirection = "asc" | "desc";

export interface SortRule<Field extends string = string> {
  field: Field;
  direction: SortDirection;
}

export interface CandidateListQuery extends PageQuery {
  job_id?: number;
  name_like?: string;
  stage?: PipelineStage;
  sorts?: SortRule<
    | "name"
    | "current_company"
    | "job_title"
    | "score"
    | "stage"
    | "years_of_experience"
    | "updated_at"
    | "created_at"
  >[];
}

export interface InterviewListQuery extends PageQuery {
  job_id?: number;
  name_like?: string;
  sorts?: SortRule<"name" | "job_title" | "stage" | "updated_at" | "created_at">[];
}

export interface DecisionListQuery extends PageQuery {
  job_id?: number;
  name_like?: string;
  interview_passed?: boolean;
  sorts?: SortRule<"name" | "job_title" | "stage" | "updated_at" | "created_at">[];
}

export type CrawlTaskPersonSyncStatus = "UNSYNCED" | "SYNCED" | "FAILED";

export interface CrawlTaskPersonRecord {
  id: number;
  task_id: number;
  source: CrawlPlatformSource;
  external_id?: string | null;
  name: string;
  current_company?: string | null;
  years_of_experience: number;
  sync_status: CrawlTaskPersonSyncStatus;
  sync_error_code?: string | null;
  sync_error_message?: string | null;
  candidate_id?: number | null;
  created_at: string;
  updated_at: string;
}

export interface StageStat {
  stage: PipelineStage;
  count: number;
}

export interface DashboardMetrics {
  total_jobs: number;
  total_candidates: number;
  total_resumes: number;
  pending_tasks: number;
  hiring_decisions_total: number;
  ai_alignment_count: number;
  ai_deviation_count: number;
  ai_alignment_rate: number;
  stage_stats: StageStat[];
}

const transitionMap: Record<PipelineStage, PipelineStage[]> = {
  NEW: ["SCREENING", "HOLD", "REJECTED"],
  SCREENING: ["INTERVIEW", "HOLD", "REJECTED"],
  INTERVIEW: ["HOLD", "REJECTED", "OFFERED"],
  HOLD: ["SCREENING", "INTERVIEW", "REJECTED"],
  REJECTED: [],
  OFFERED: [],
};

export function isValidPipelineTransition(
  from: PipelineStage,
  to: PipelineStage,
): boolean {
  return transitionMap[from].includes(to);
}

function clampScore(value: number): number {
  return Math.max(0, Math.min(100, Math.round(value)));
}

export function buildScorecard(
  profile: CandidateProfile,
  job: JobContext,
): AnalysisResult {
  const skillSet = new Set(profile.skills.map((item) => item.toLowerCase()));

  const matchedMust = job.mustHaveSkills.filter((skill) =>
    skillSet.has(skill.toLowerCase()),
  );
  const matchedNice = job.niceToHaveSkills.filter((skill) =>
    skillSet.has(skill.toLowerCase()),
  );

  const mustCoverage =
    job.mustHaveSkills.length === 0
      ? 1
      : matchedMust.length / job.mustHaveSkills.length;
  const niceCoverage =
    job.niceToHaveSkills.length === 0
      ? 0
      : matchedNice.length / job.niceToHaveSkills.length;

  const skillScore = clampScore(mustCoverage * 85 + niceCoverage * 15);

  const experienceGap = profile.basic.yearsOfExperience - job.minYears;
  const experienceScore = clampScore(70 + experienceGap * 8);

  const hasSalaryLimit = typeof job.maxSalaryK === "number";
  const hasCandidateSalary = typeof profile.basic.expectedSalaryK === "number";
  let compensationScore = 80;
  if (hasSalaryLimit && hasCandidateSalary) {
    const salaryDelta = (job.maxSalaryK as number) - (profile.basic.expectedSalaryK as number);
    compensationScore = clampScore(80 + salaryDelta * 3);
  }

  const totalYears = profile.experiences.reduce((sum, item) => sum + item.years, 0);
  const averageTenure =
    profile.experiences.length === 0 ? profile.basic.yearsOfExperience : totalYears / profile.experiences.length;
  const stabilityScore = clampScore(55 + averageTenure * 15);

  const dimensionScores: DimensionScore[] = [
    {
      key: "skill_match",
      score: skillScore,
      reason: `Must-have matched ${matchedMust.length}/${job.mustHaveSkills.length}; nice-to-have matched ${matchedNice.length}/${job.niceToHaveSkills.length}.`,
    },
    {
      key: "experience",
      score: experienceScore,
      reason: `Candidate years ${profile.basic.yearsOfExperience} vs required ${job.minYears}.`,
    },
    {
      key: "compensation",
      score: compensationScore,
      reason: hasSalaryLimit && hasCandidateSalary
        ? `Expected ${profile.basic.expectedSalaryK}k vs budget ${job.maxSalaryK}k.`
        : "Compensation data incomplete, using neutral baseline.",
    },
    {
      key: "stability",
      score: stabilityScore,
      reason: `Average tenure is ${averageTenure.toFixed(1)} years across ${profile.experiences.length} experiences.`,
    },
  ];

  const overallScore = clampScore(
    skillScore * 0.4 +
      experienceScore * 0.25 +
      compensationScore * 0.15 +
      stabilityScore * 0.2,
  );

  const risks: string[] = [];
  if (mustCoverage < 1) {
    risks.push(
      `Missing ${job.mustHaveSkills.length - matchedMust.length} must-have skills for this role.`,
    );
  }
  if (
    hasSalaryLimit &&
    hasCandidateSalary &&
    (profile.basic.expectedSalaryK as number) > (job.maxSalaryK as number)
  ) {
    risks.push("Expected salary exceeds configured job budget.");
  }
  if (averageTenure < 1.5) {
    risks.push("Job hopping risk: average tenure below 1.5 years.");
  }

  const highlights: string[] = [];
  if (matchedMust.length > 0) {
    highlights.push(`Matched must-have skills: ${matchedMust.join(", ")}.`);
  }
  if (profile.basic.yearsOfExperience >= job.minYears) {
    highlights.push("Experience meets or exceeds baseline requirement.");
  }
  if (averageTenure >= 2) {
    highlights.push("Stable tenure pattern based on historical roles.");
  }

  const suggestions: string[] = [];
  if (mustCoverage < 1) {
    suggestions.push("安排一次技术面，重点验证缺失技能的可迁移能力。")
  }
  if (compensationScore < 60) {
    suggestions.push("建议尽早沟通薪资区间与可接受条件。")
  }
  if (stabilityScore < 60) {
    suggestions.push("建议补充离职原因与职业规划问题。")
  }
  if (suggestions.length === 0) {
    suggestions.push("可优先进入初筛面试，验证文化契合度与到岗时间。")
  }

  const evidence: EvidenceItem[] = [
    {
      dimension: "skill_match",
      statement: `Matched must-have skills: ${matchedMust.join(", ") || "none"}`,
      sourceSnippet: profile.rawText.slice(0, 120),
    },
    {
      dimension: "experience",
      statement: `Total years: ${profile.basic.yearsOfExperience}`,
      sourceSnippet: profile.rawText.slice(0, 120),
    },
    {
      dimension: "stability",
      statement: `Average tenure ${averageTenure.toFixed(1)} years`,
      sourceSnippet: JSON.stringify(profile.experiences).slice(0, 120),
    },
  ];

  return {
    overallScore,
    dimensionScores,
    risks,
    highlights,
    suggestions,
    evidence,
    modelInfo: {
      provider: "local-heuristic",
      model: "local-scorecard-v1",
      generatedAt: new Date().toISOString(),
    },
  };
}
