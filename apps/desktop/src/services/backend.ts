import { invoke } from "@tauri-apps/api/core";
import type {
  CandidateListQuery,
  CandidateGender,
  CandidateRecord,
  CrawlTaskScheduleType,
  CrawlPlatformSource,
  CrawlMode,
  CrawlTaskPersonRecord,
  CrawlTaskRecord,
  CrawlTaskSource,
  DecisionListQuery,
  DashboardMetrics,
  HiringDecision,
  HiringFinalDecision,
  InterviewListQuery,
  InterviewQuestion,
  InterviewRecommendation,
  JobRecord,
  PageResult,
  PipelineStage,
  ResumeRecord,
  SourceType,
} from "@doss/shared";

export interface NewJobPayload {
  source?: SourceType;
  external_id?: string;
  title: string;
  company: string;
  city?: string;
  salary_k?: string;
  description?: string;
}

export interface UpdateJobPayload {
  job_id: number;
  title: string;
  company: string;
  city?: string;
  salary_k?: string;
  description?: string;
}

export interface NewCandidatePayload {
  source?: SourceType;
  external_id?: string;
  name: string;
  current_company?: string;
  score?: number;
  age?: number;
  gender?: CandidateGender;
  years_of_experience: number;
  address?: string;
  phone?: string;
  email?: string;
  tags: string[];
  job_id?: number;
}

export interface UpdateCandidatePayload {
  candidate_id: number;
  name: string;
  current_company?: string;
  job_id?: number | null;
  score?: number;
  age?: number;
  gender?: CandidateGender;
  years_of_experience: number;
  address?: string;
  phone?: string;
  email?: string;
  tags: string[];
}

export interface MergeCandidateImportPayload {
  candidate_id: number;
  current_company?: string;
  years_of_experience?: number;
  address?: string;
  tags?: string[];
  phone?: string;
  email?: string;
  job_id?: number;
}

export interface MoveStagePayload {
  candidate_id: number;
  job_id?: number;
  to_stage: PipelineStage;
  note?: string;
}

export interface SetCandidateQualificationPayload {
  candidate_id: number;
  qualified: boolean;
  note?: string;
}

export interface UpsertResumePayload {
  candidate_id: number;
  source?: SourceType;
  raw_text: string;
  parsed: Record<string, unknown>;
}

export interface ParseResumeFilePayload {
  file_name: string;
  content_base64: string;
  enable_ocr?: boolean;
}

export interface ParsedResumeFile {
  raw_text: string;
  parsed: Record<string, unknown>;
  metadata: Record<string, unknown>;
}

export interface CrawlTaskPayload {
  source: CrawlTaskSource;
  mode: CrawlMode;
  task_type: string;
  payload: Record<string, unknown>;
  schedule_type?: CrawlTaskScheduleType;
  schedule_time?: string;
  schedule_day?: number;
  next_run_at?: string;
}

export interface UpsertCrawlTaskPersonPayload {
  source: CrawlPlatformSource;
  external_id?: string;
  name: string;
  current_company?: string;
  years_of_experience: number;
  sync_status?: "UNSYNCED" | "SYNCED" | "FAILED";
  sync_error_code?: string;
  sync_error_message?: string;
  candidate_id?: number;
}

export interface UpsertCrawlTaskPeoplePayload {
  task_id: number;
  people: UpsertCrawlTaskPersonPayload[];
}

export interface CrawlTaskPersonSyncUpdatePayload {
  person_id: number;
  sync_status: "UNSYNCED" | "SYNCED" | "FAILED";
  sync_error_code?: string;
  sync_error_message?: string;
  candidate_id?: number;
}

export interface UpdateCrawlTaskPeopleSyncPayload {
  task_id: number;
  updates: CrawlTaskPersonSyncUpdatePayload[];
}

export interface UpdateTaskPayload {
  task_id: number;
  status: "PENDING" | "RUNNING" | "PAUSED" | "CANCELED" | "SUCCEEDED" | "FAILED";
  retry_count?: number;
  error_code?: string;
  snapshot?: Record<string, unknown>;
  schedule_type?: CrawlTaskScheduleType;
  schedule_time?: string;
  schedule_day?: number;
  next_run_at?: string;
}

export interface UpsertPendingCandidatePayload {
  source?: SourceType;
  external_id?: string;
  name: string;
  current_company?: string;
  job_id?: number;
  age?: number;
  gender?: CandidateGender;
  years_of_experience?: number;
  tags?: string[];
  phone?: string;
  email?: string;
  address?: string;
  extra_notes?: string;
  resume_raw_text?: string;
  resume_parsed?: Record<string, unknown>;
  dedupe_key?: string;
}

export interface UpsertPendingCandidatesPayload {
  items: UpsertPendingCandidatePayload[];
}

export interface PendingCandidateRecord {
  id: number;
  source: string;
  external_id?: string | null;
  name: string;
  current_company?: string | null;
  job_id?: number | null;
  job_title?: string | null;
  age?: number | null;
  gender?: CandidateGender | null;
  years_of_experience: number;
  tags: string[];
  phone_masked?: string | null;
  email_masked?: string | null;
  address?: string | null;
  extra_notes?: string | null;
  resume_raw_text?: string | null;
  resume_parsed: Record<string, unknown>;
  dedupe_key: string;
  sync_status: "UNSYNCED" | "SYNCED" | "FAILED";
  sync_error_code?: string | null;
  sync_error_message?: string | null;
  candidate_id?: number | null;
  created_at: string;
  updated_at: string;
}

export interface PendingCandidateListQuery {
  page?: number;
  page_size?: number;
  sync_status?: "UNSYNCED" | "SYNCED" | "FAILED";
  name_like?: string;
  job_id?: number;
}

export interface SyncPendingCandidatePayload {
  pending_candidate_id: number;
  run_screening?: boolean;
}

export type AiProviderId =
  | "qwen"
  | "doubao"
  | "deepseek"
  | "minimax"
  | "glm"
  | "openapi";

export interface AiProviderSettings {
  provider: AiProviderId;
  model: string;
  base_url: string;
  temperature: number;
  max_tokens: number;
  timeout_secs: number;
  retry_count: number;
  has_api_key: boolean;
}

export interface UpsertAiProviderSettingsPayload {
  provider: AiProviderId;
  model?: string;
  base_url?: string;
  temperature?: number;
  max_tokens?: number;
  timeout_secs?: number;
  retry_count?: number;
  api_key?: string;
}

export interface AiProviderCatalogItem {
  id: AiProviderId;
  label: string;
  default_model: string;
  default_base_url: string;
  models: string[];
  docs: string[];
}

export interface AiProviderCatalog {
  providers: AiProviderCatalogItem[];
  updated_at: string;
}

export interface AiProviderTestResult {
  ok: boolean;
  provider: AiProviderId;
  model: string;
  endpoint: string;
  latency_ms: number;
  reply_excerpt: string;
  tested_at: string;
}

export interface AiProviderProfile {
  id: string;
  name: string;
  provider: AiProviderId;
  model: string;
  base_url: string;
  temperature: number;
  max_tokens: number;
  timeout_secs: number;
  retry_count: number;
  has_api_key: boolean;
  is_active: boolean;
  created_at: string;
  updated_at: string;
}

export interface UpsertAiProviderProfilePayload {
  profile_id?: string;
  name?: string;
  provider: AiProviderId;
  model?: string;
  base_url?: string;
  temperature?: number;
  max_tokens?: number;
  timeout_secs?: number;
  retry_count?: number;
  api_key?: string;
}

export interface PipelineEvent {
  id: number;
  candidate_id: number;
  job_id?: number;
  from_stage: PipelineStage;
  to_stage: PipelineStage;
  note?: string;
  created_at: string;
}

export interface SearchHit {
  candidate_id: number;
  name: string;
  stage: PipelineStage;
  snippet: string;
}

export interface SidecarQueueResult {
  id: string;
  source: string;
  mode: CrawlMode;
  status: "SUCCEEDED" | "FAILED" | "SKIPPED_DUPLICATE";
  attempts: number;
  output?: unknown;
  error?: string;
  errorCode?: string;
  snapshot?: Record<string, unknown>;
}

export interface AppHealth {
  ok: boolean;
  dbPath: string;
  dbExists: boolean;
  schemaVersion: number;
}

export interface SidecarRuntime {
  ok: boolean;
  port: number;
  base_url: string;
  source: string;
  message?: string;
  restart_count: number;
}

export interface TaskRuntimeSettings {
  auto_batch_concurrency: number;
  auto_retry_count: number;
  auto_retry_backoff_ms: number;
}

export interface UpdateTaskRuntimeSettingsPayload {
  auto_batch_concurrency?: number;
  auto_retry_count?: number;
  auto_retry_backoff_ms?: number;
}

export interface BackendAnalysisRecord {
  id: number;
  candidate_id: number;
  job_id?: number;
  overall_score: number;
  dimension_scores: Array<{
    key: string;
    score: number;
    reason: string;
  }>;
  risks: string[];
  highlights: string[];
  suggestions: string[];
  evidence: Array<{
    dimension: string;
    statement: string;
    source_snippet: string;
  }>;
  model_info: Record<string, unknown>;
  created_at: string;
}

export type ScoringRecommendation = "PASS" | "REVIEW" | "REJECT";
export type ScoringRiskLevel = "LOW" | "MEDIUM" | "HIGH";

export interface ScoringItemConfig {
  key: string;
  label: string;
  description: string;
  weight: number;
}

export interface ScoringSectionConfig {
  items: ScoringItemConfig[];
}

export interface ScoringTemplateConfig {
  weights: {
    t0: number;
    t1: number;
    t2: number;
    t3: number;
  };
  t0: ScoringSectionConfig;
  t1: ScoringSectionConfig;
  t2: ScoringSectionConfig;
  t3: ScoringSectionConfig;
}

export interface ScoringTemplateRecord {
  id: number;
  scope: "global" | "job";
  name: string;
  job_id?: number | null;
  config: ScoringTemplateConfig;
  created_at: string;
  updated_at: string;
}

export interface UpsertScoringTemplatePayload {
  job_id?: number;
  name?: string;
  config?: ScoringTemplateConfig;
}

export interface CreateScoringTemplatePayload {
  name?: string;
  config?: ScoringTemplateConfig;
}

export interface UpdateScoringTemplatePayload {
  template_id: number;
  name?: string;
  config?: ScoringTemplateConfig;
}

export interface SetJobScoringTemplatePayload {
  job_id: number;
  template_id?: number | null;
}

export interface ScoringResultRecord {
  id: number;
  candidate_id: number;
  job_id?: number | null;
  template_id?: number | null;
  overall_score: number;
  overall_score_5: number;
  t0_score_5: number;
  t1_score_5: number;
  t2_score_5: number;
  t3_score_5: number;
  recommendation: ScoringRecommendation;
  risk_level: ScoringRiskLevel;
  structured_result: Record<string, unknown>;
  created_at: string;
}

// Backward compatibility types for modules/tests still migrating from screening naming.
export type ScreeningRecommendation = ScoringRecommendation;
export type ScreeningRiskLevel = ScoringRiskLevel;
export interface ScreeningDimension {
  key: string;
  label: string;
  weight: number;
}
export interface ScreeningTemplateRecord {
  id: number;
  scope: "global" | "job";
  name: string;
  job_id?: number | null;
  dimensions: ScreeningDimension[];
  risk_rules: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}
export interface UpsertScreeningTemplatePayload {
  job_id?: number;
  name?: string;
  dimensions?: ScreeningDimension[];
  risk_rules?: Record<string, unknown>;
  config?: ScoringTemplateConfig;
}
export interface CreateScreeningTemplatePayload {
  name?: string;
  dimensions?: ScreeningDimension[];
  risk_rules?: Record<string, unknown>;
  config?: ScoringTemplateConfig;
}
export interface UpdateScreeningTemplatePayload {
  template_id: number;
  name?: string;
  dimensions?: ScreeningDimension[];
  risk_rules?: Record<string, unknown>;
  config?: ScoringTemplateConfig;
}
export type SetJobScreeningTemplatePayload = SetJobScoringTemplatePayload;
export type ScreeningResultRecord = ScoringResultRecord;

export interface GenerateInterviewKitPayload {
  candidate_id: number;
  job_id?: number;
}

export interface SaveInterviewKitPayload {
  candidate_id: number;
  job_id?: number;
  questions: InterviewQuestion[];
}

export interface SubmitInterviewFeedbackPayload {
  candidate_id: number;
  job_id?: number;
  transcript_text: string;
  structured_feedback: Record<string, unknown>;
  recording_path?: string;
}

export interface RunInterviewEvaluationPayload {
  candidate_id: number;
  job_id?: number;
  feedback_id?: number;
}

export interface SaveInterviewRecordingPayload {
  file_name: string;
  content_base64: string;
}

export interface SaveInterviewRecordingOutput {
  recording_path: string;
  size: number;
  created_at: string;
}

export interface InterviewKitRecord {
  id?: number | null;
  candidate_id: number;
  job_id?: number | null;
  questions: InterviewQuestion[];
  generated_by: string;
  created_at: string;
  updated_at: string;
}

export interface InterviewFeedbackRecord {
  id: number;
  candidate_id: number;
  job_id?: number | null;
  transcript_text: string;
  structured_feedback: Record<string, unknown>;
  recording_path?: string | null;
  created_at: string;
  updated_at: string;
}

export interface InterviewEvaluationRecord {
  id: number;
  candidate_id: number;
  job_id?: number | null;
  feedback_id: number;
  recommendation: InterviewRecommendation;
  overall_score: number;
  confidence: number;
  evidence: string[];
  verification_points: string[];
  uncertainty: string;
  created_at: string;
}

export interface FinalizeHiringDecisionPayload {
  candidate_id: number;
  job_id?: number;
  final_decision: HiringFinalDecision;
  reason_code: string;
  note?: string;
}

export type HiringDecisionRecord = HiringDecision;

export async function getHealth(): Promise<AppHealth> {
  return invoke<AppHealth>("app_health");
}

export async function listJobs(): Promise<JobRecord[]> {
  return invoke<JobRecord[]>("list_jobs");
}

export async function createJob(input: NewJobPayload): Promise<JobRecord> {
  return invoke<JobRecord>("create_job", { input });
}

export async function updateJob(input: UpdateJobPayload): Promise<JobRecord> {
  return invoke<JobRecord>("update_job", { input });
}

export async function stopJob(job_id: number): Promise<JobRecord> {
  return invoke<JobRecord>("stop_job", { job_id });
}

export async function deleteJob(job_id: number): Promise<boolean> {
  return invoke<boolean>("delete_job", { jobId: job_id });
}

export async function listCandidates(stage?: PipelineStage): Promise<CandidateRecord[]> {
  return invoke<CandidateRecord[]>("list_candidates", { stage });
}

export async function listCandidatesPage(
  input?: CandidateListQuery,
): Promise<PageResult<CandidateRecord>> {
  return invoke<PageResult<CandidateRecord>>("list_candidates_page", { input });
}

export async function listInterviewCandidatesPage(
  input?: InterviewListQuery,
): Promise<PageResult<CandidateRecord>> {
  return invoke<PageResult<CandidateRecord>>("list_interview_candidates_page", { input });
}

export async function listDecisionCandidatesPage(
  input?: DecisionListQuery,
): Promise<PageResult<CandidateRecord>> {
  return invoke<PageResult<CandidateRecord>>("list_decision_candidates_page", { input });
}

export async function createCandidate(input: NewCandidatePayload): Promise<CandidateRecord> {
  return invoke<CandidateRecord>("create_candidate", { input });
}

export async function updateCandidate(input: UpdateCandidatePayload): Promise<CandidateRecord> {
  return invoke<CandidateRecord>("update_candidate", { input });
}

export async function deleteCandidate(candidate_id: number): Promise<boolean> {
  return invoke<boolean>("delete_candidate", { candidateId: candidate_id });
}

export async function setCandidateQualification(input: SetCandidateQualificationPayload): Promise<CandidateRecord> {
  return invoke<CandidateRecord>("set_candidate_qualification", { input });
}

export async function mergeCandidateImport(input: MergeCandidateImportPayload): Promise<CandidateRecord> {
  return invoke<CandidateRecord>("merge_candidate_import", { input });
}

export async function moveCandidateStage(input: MoveStagePayload): Promise<PipelineEvent> {
  return invoke<PipelineEvent>("move_candidate_stage", { input });
}

export async function listPipelineEvents(candidateId: number): Promise<PipelineEvent[]> {
  return invoke<PipelineEvent[]>("list_pipeline_events", {
    candidateId,
  });
}

export async function upsertResume(input: UpsertResumePayload): Promise<ResumeRecord> {
  return invoke<ResumeRecord>("upsert_resume", { input });
}

export async function parseResumeFile(input: ParseResumeFilePayload): Promise<ParsedResumeFile> {
  return invoke<ParsedResumeFile>("parse_resume_file", { input });
}

export async function runCandidateAnalysis(input: {
  candidate_id: number;
  job_id?: number;
  run_id?: string;
}): Promise<ScoringResultRecord> {
  try {
    return await invoke<ScoringResultRecord>("run_candidate_analysis", { input });
  } catch {
    return invoke<ScoringResultRecord>("run_candidate_scoring", { input });
  }
}

export async function getScoringTemplate(job_id?: number): Promise<ScoringTemplateRecord> {
  return invoke<ScoringTemplateRecord>("get_scoring_template", { job_id });
}

export async function upsertScoringTemplate(
  input: UpsertScoringTemplatePayload,
): Promise<ScoringTemplateRecord> {
  return invoke<ScoringTemplateRecord>("upsert_scoring_template", { input });
}

export async function listScoringTemplates(): Promise<ScoringTemplateRecord[]> {
  return invoke<ScoringTemplateRecord[]>("list_scoring_templates");
}

export async function createScoringTemplate(
  input: CreateScoringTemplatePayload,
): Promise<ScoringTemplateRecord> {
  return invoke<ScoringTemplateRecord>("create_scoring_template", { input });
}

export async function updateScoringTemplate(
  input: UpdateScoringTemplatePayload,
): Promise<ScoringTemplateRecord> {
  return invoke<ScoringTemplateRecord>("update_scoring_template", { input });
}

export async function deleteScoringTemplate(template_id: number): Promise<ScoringTemplateRecord[]> {
  return invoke<ScoringTemplateRecord[]>("delete_scoring_template", { templateId: template_id });
}

export async function setJobScoringTemplate(input: SetJobScoringTemplatePayload): Promise<JobRecord> {
  return invoke<JobRecord>("set_job_scoring_template", { input });
}

export async function runCandidateScoring(input: {
  candidate_id: number;
  job_id?: number;
  run_id?: string;
}): Promise<ScoringResultRecord> {
  return invoke<ScoringResultRecord>("run_candidate_scoring", { input });
}

export async function listScoringResults(candidate_id: number): Promise<ScoringResultRecord[]> {
  return invoke<ScoringResultRecord[]>("list_scoring_results", { candidateId: candidate_id });
}

function createDefaultScoringConfig(): ScoringTemplateConfig {
  return {
    weights: { t0: 50, t1: 30, t2: 10, t3: 10 },
    t0: {
      items: [
        { key: "required_skills_match", label: "岗位技能匹配", description: "岗位描述/技能要求与候选人技能覆盖是否匹配。", weight: 50 },
        { key: "years_experience_match", label: "经验年限匹配", description: "候选人年限是否满足岗位复杂度要求。", weight: 30 },
        { key: "resume_completeness", label: "简历信息完整度", description: "简历证据是否足以支撑判断。", weight: 20 },
      ],
    },
    t1: {
      items: [
        { key: "goal_orientation", label: "目标导向", description: "是否有明确目标并形成可交付结果。", weight: 30 },
        { key: "team_collaboration", label: "团队协作", description: "跨角色协作、沟通与推进效率。", weight: 15 },
        { key: "self_drive", label: "自驱力", description: "主动承担、持续推进和问题闭环能力。", weight: 15 },
        { key: "reflection_iteration", label: "反思迭代", description: "复盘意识和迭代改进能力。", weight: 10 },
        { key: "openness", label: "开放性", description: "对反馈与变化的接受度和执行力。", weight: 8 },
        { key: "resilience", label: "抗压韧性", description: "复杂场景下的稳定性和恢复能力。", weight: 7 },
        { key: "learning_ability", label: "学习能力", description: "知识吸收与迁移速度。", weight: 10 },
        { key: "values_fit", label: "价值观契合", description: "与团队协作价值观一致性。", weight: 5 },
      ],
    },
    t2: {
      items: [
        { key: "core_skill_bonus", label: "核心技能加分", description: "核心技能命中程度是否超出岗位最低要求。", weight: 40 },
        { key: "project_impact_bonus", label: "项目影响力加分", description: "项目成果是否有可量化业务影响。", weight: 30 },
        { key: "rare_stack_bonus", label: "稀缺技术栈加分", description: "是否具备岗位稀缺/高价值技术栈。", weight: 30 },
      ],
    },
    t3: {
      items: [
        { key: "salary_risk", label: "薪资风险", description: "薪资预期与岗位预算差异风险（低风险高分）。", weight: 35 },
        { key: "stability_risk", label: "稳定性风险", description: "履历稳定性与持续投入风险（低风险高分）。", weight: 35 },
        { key: "info_completeness_risk", label: "信息缺失风险", description: "关键信息缺失带来的决策风险（低风险高分）。", weight: 30 },
      ],
    },
  };
}

function toLegacyTemplateRecord(record: ScoringTemplateRecord): ScreeningTemplateRecord {
  const unknownRecord = record as unknown as {
    dimensions?: ScreeningDimension[];
    risk_rules?: Record<string, unknown>;
  };
  const dimensions = record.config?.t1?.items?.length
    ? record.config.t1.items.map((item) => ({
        key: item.key,
        label: item.label,
        weight: item.weight,
      }))
    : (unknownRecord.dimensions ?? []);
  return {
    id: record.id,
    scope: record.scope,
    name: record.name,
    job_id: record.job_id,
    dimensions,
    risk_rules: unknownRecord.risk_rules ?? {},
    created_at: record.created_at,
    updated_at: record.updated_at,
  };
}

function toScoringTemplateInputFromLegacy(
  input: {
    name?: string;
    job_id?: number;
    dimensions?: ScreeningDimension[];
    config?: ScoringTemplateConfig;
  },
): {
  name?: string;
  job_id?: number;
  config?: ScoringTemplateConfig;
} {
  if (input.config) {
    return { name: input.name, job_id: input.job_id, config: input.config };
  }
  const config = createDefaultScoringConfig();
  if (Array.isArray(input.dimensions) && input.dimensions.length > 0) {
    config.t1.items = input.dimensions.map((dimension) => ({
      key: dimension.key,
      label: dimension.label,
      description: "",
      weight: dimension.weight,
    }));
  }
  return { name: input.name, job_id: input.job_id, config };
}

// Backward compatibility wrappers.
export async function getScreeningTemplate(job_id?: number): Promise<ScreeningTemplateRecord> {
  try {
    return await invoke<ScreeningTemplateRecord>("get_screening_template", { job_id });
  } catch {
    return toLegacyTemplateRecord(await getScoringTemplate(job_id));
  }
}

export async function upsertScreeningTemplate(
  input: UpsertScreeningTemplatePayload,
): Promise<ScreeningTemplateRecord> {
  try {
    return await invoke<ScreeningTemplateRecord>("upsert_screening_template", { input });
  } catch {
    const next = toScoringTemplateInputFromLegacy(input);
    return toLegacyTemplateRecord(await upsertScoringTemplate(next));
  }
}

export async function listScreeningTemplates(): Promise<ScreeningTemplateRecord[]> {
  try {
    return await invoke<ScreeningTemplateRecord[]>("list_screening_templates");
  } catch {
    return (await listScoringTemplates()).map(toLegacyTemplateRecord);
  }
}

export async function createScreeningTemplate(
  input: CreateScreeningTemplatePayload,
): Promise<ScreeningTemplateRecord> {
  try {
    return await invoke<ScreeningTemplateRecord>("create_screening_template", { input });
  } catch {
    const next = toScoringTemplateInputFromLegacy(input);
    return toLegacyTemplateRecord(await createScoringTemplate(next));
  }
}

export async function updateScreeningTemplate(
  input: UpdateScreeningTemplatePayload,
): Promise<ScreeningTemplateRecord> {
  try {
    return await invoke<ScreeningTemplateRecord>("update_screening_template", { input });
  } catch {
    const next = {
      template_id: input.template_id,
      name: input.name,
      config: toScoringTemplateInputFromLegacy(input).config,
    };
    return toLegacyTemplateRecord(await updateScoringTemplate(next));
  }
}

export async function deleteScreeningTemplate(template_id: number): Promise<ScreeningTemplateRecord[]> {
  try {
    return await invoke<ScreeningTemplateRecord[]>("delete_screening_template", { templateId: template_id });
  } catch {
    return (await deleteScoringTemplate(template_id)).map(toLegacyTemplateRecord);
  }
}

export async function setJobScreeningTemplate(input: SetJobScreeningTemplatePayload): Promise<JobRecord> {
  try {
    return await invoke<JobRecord>("set_job_screening_template", { input });
  } catch {
    return setJobScoringTemplate(input);
  }
}

export async function runResumeScreening(input: {
  candidate_id: number;
  job_id?: number;
}): Promise<ScoringResultRecord> {
  try {
    return await invoke<ScoringResultRecord>("run_resume_screening", { input });
  } catch {
    return runCandidateScoring(input);
  }
}

export async function listScreeningResults(candidate_id: number): Promise<ScoringResultRecord[]> {
  try {
    return await invoke<ScoringResultRecord[]>("list_screening_results", { candidateId: candidate_id });
  } catch {
    return listScoringResults(candidate_id);
  }
}

export async function listHiringDecisions(candidate_id: number): Promise<HiringDecisionRecord[]> {
  return invoke<HiringDecisionRecord[]>("list_hiring_decisions", { candidateId: candidate_id });
}

export async function generateInterviewKit(
  input: GenerateInterviewKitPayload,
): Promise<InterviewKitRecord> {
  return invoke<InterviewKitRecord>("generate_interview_kit", { input });
}

export async function saveInterviewKit(
  input: SaveInterviewKitPayload,
): Promise<InterviewKitRecord> {
  return invoke<InterviewKitRecord>("save_interview_kit", { input });
}

export async function submitInterviewFeedback(
  input: SubmitInterviewFeedbackPayload,
): Promise<InterviewFeedbackRecord> {
  return invoke<InterviewFeedbackRecord>("submit_interview_feedback", { input });
}

export async function saveInterviewRecording(
  input: SaveInterviewRecordingPayload,
): Promise<SaveInterviewRecordingOutput> {
  return invoke<SaveInterviewRecordingOutput>("save_interview_recording", { input });
}

export async function runInterviewEvaluation(
  input: RunInterviewEvaluationPayload,
): Promise<InterviewEvaluationRecord> {
  return invoke<InterviewEvaluationRecord>("run_interview_evaluation", { input });
}

export async function listInterviewEvaluations(candidate_id: number): Promise<InterviewEvaluationRecord[]> {
  return invoke<InterviewEvaluationRecord[]>("list_interview_evaluations", { candidateId: candidate_id });
}

export async function finalizeHiringDecision(
  input: FinalizeHiringDecisionPayload,
): Promise<HiringDecisionRecord> {
  return invoke<HiringDecisionRecord>("finalize_hiring_decision", { input });
}

export async function getAiProviderSettings(): Promise<AiProviderSettings> {
  return invoke<AiProviderSettings>("get_ai_provider_settings");
}

export async function getAiProviderCatalog(): Promise<AiProviderCatalog> {
  return invoke<AiProviderCatalog>("get_ai_provider_catalog");
}

export async function listAiProviderProfiles(): Promise<AiProviderProfile[]> {
  return invoke<AiProviderProfile[]>("list_ai_provider_profiles");
}

export async function upsertAiProviderProfile(
  input: UpsertAiProviderProfilePayload,
): Promise<AiProviderProfile> {
  return invoke<AiProviderProfile>("upsert_ai_provider_profile", { input });
}

export async function deleteAiProviderProfile(profile_id: string): Promise<AiProviderProfile[]> {
  return invoke<AiProviderProfile[]>("delete_ai_provider_profile", { profileId: profile_id });
}

export async function setDefaultAiProviderProfile(profile_id: string): Promise<AiProviderProfile[]> {
  return invoke<AiProviderProfile[]>("set_default_ai_provider_profile", { profileId: profile_id });
}

export async function testAiProviderProfile(profile_id: string): Promise<AiProviderTestResult> {
  return invoke<AiProviderTestResult>("test_ai_provider_profile", { profileId: profile_id });
}

export async function upsertAiProviderSettings(
  input: UpsertAiProviderSettingsPayload,
): Promise<AiProviderSettings> {
  return invoke<AiProviderSettings>("upsert_ai_provider_settings", { input });
}

export async function testAiProviderSettings(
  input: UpsertAiProviderSettingsPayload,
): Promise<AiProviderTestResult> {
  return invoke<AiProviderTestResult>("test_ai_provider_settings", { input });
}

export async function listAnalysis(candidateId: number): Promise<BackendAnalysisRecord[]> {
  return invoke<BackendAnalysisRecord[]>("list_analysis", {
    candidateId,
  });
}

export async function createCrawlTask(input: CrawlTaskPayload): Promise<CrawlTaskRecord> {
  return invoke<CrawlTaskRecord>("create_crawl_task", { input });
}

export async function updateCrawlTask(input: UpdateTaskPayload): Promise<CrawlTaskRecord> {
  return invoke<CrawlTaskRecord>("update_crawl_task", { input });
}

export async function listCrawlTasks(): Promise<CrawlTaskRecord[]> {
  return invoke<CrawlTaskRecord[]>("list_crawl_tasks");
}

export async function deleteCrawlTask(task_id: number): Promise<boolean> {
  return invoke<boolean>("delete_crawl_task", { taskId: task_id });
}

export async function upsertCrawlTaskPeople(
  input: UpsertCrawlTaskPeoplePayload,
): Promise<CrawlTaskPersonRecord[]> {
  return invoke<CrawlTaskPersonRecord[]>("upsert_crawl_task_people", { input });
}

export async function listCrawlTaskPeople(task_id: number): Promise<CrawlTaskPersonRecord[]> {
  return invoke<CrawlTaskPersonRecord[]>("list_crawl_task_people", { taskId: task_id });
}

export async function updateCrawlTaskPeopleSync(
  input: UpdateCrawlTaskPeopleSyncPayload,
): Promise<CrawlTaskPersonRecord[]> {
  return invoke<CrawlTaskPersonRecord[]>("update_crawl_task_people_sync", { input });
}

export async function upsertPendingCandidates(
  input: UpsertPendingCandidatesPayload,
): Promise<PendingCandidateRecord[]> {
  return invoke<PendingCandidateRecord[]>("upsert_pending_candidates", { input });
}

export async function listPendingCandidates(
  input?: PendingCandidateListQuery,
): Promise<PageResult<PendingCandidateRecord>> {
  return invoke<PageResult<PendingCandidateRecord>>("list_pending_candidates", { input });
}

export async function syncPendingCandidateToCandidate(
  input: SyncPendingCandidatePayload,
): Promise<CandidateRecord> {
  return invoke<CandidateRecord>("sync_pending_candidate_to_candidate", { input });
}

export async function getTaskRuntimeSettings(): Promise<TaskRuntimeSettings> {
  return invoke<TaskRuntimeSettings>("get_task_runtime_settings");
}

export async function upsertTaskRuntimeSettings(
  input: UpdateTaskRuntimeSettingsPayload,
): Promise<TaskRuntimeSettings> {
  return invoke<TaskRuntimeSettings>("upsert_task_runtime_settings", { input });
}

export async function searchCandidates(query: string): Promise<SearchHit[]> {
  return invoke<SearchHit[]>("search_candidates", { query });
}

export async function loadDashboardMetrics(): Promise<DashboardMetrics> {
  return invoke<DashboardMetrics>("dashboard_metrics");
}

export async function ensureSidecar(): Promise<SidecarRuntime> {
  const runtime = await invoke<SidecarRuntime>("ensure_sidecar");
  if (!runtime.ok) {
    throw new Error(runtime.message || "Sidecar unavailable");
  }
  return runtime;
}

async function invokeSidecar<T>(command: string, input?: Record<string, unknown>): Promise<T> {
  if (input) {
    return invoke<T>(command, { input });
  }
  return invoke<T>(command);
}

export async function checkSidecarHealth(): Promise<{ ok: boolean; service?: string }> {
  return invokeSidecar<{ ok: boolean; service?: string }>("sidecar_health");
}

export async function triggerSidecarCrawlJobs(payload: {
  source: CrawlPlatformSource;
  mode: CrawlMode;
  keyword: string;
  city?: string;
}): Promise<SidecarQueueResult> {
  return invokeSidecar<SidecarQueueResult>("sidecar_crawl_jobs", {
    source: payload.source,
    mode: payload.mode,
    keyword: payload.keyword,
    city: payload.city,
  });
}

export async function triggerSidecarCrawlCandidates(payload: {
  source: CrawlPlatformSource;
  mode: CrawlMode;
  jobId: string;
}): Promise<SidecarQueueResult> {
  return invokeSidecar<SidecarQueueResult>("sidecar_crawl_candidates", {
    source: payload.source,
    mode: payload.mode,
    job_id: payload.jobId,
  });
}

export async function triggerSidecarCrawlResume(payload: {
  source: CrawlPlatformSource;
  mode: CrawlMode;
  candidateId: string;
}): Promise<SidecarQueueResult> {
  return invokeSidecar<SidecarQueueResult>("sidecar_crawl_resume", {
    source: payload.source,
    mode: payload.mode,
    candidate_id: payload.candidateId,
  });
}
