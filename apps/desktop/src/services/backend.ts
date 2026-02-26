import { invoke } from "@tauri-apps/api/core";
import type {
  CandidateRecord,
  CrawlMode,
  CrawlTaskRecord,
  DashboardMetrics,
  HiringDecision,
  HiringFinalDecision,
  InterviewQuestion,
  InterviewRecommendation,
  JobRecord,
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

export interface NewCandidatePayload {
  source?: SourceType;
  external_id?: string;
  name: string;
  current_company?: string;
  years_of_experience: number;
  phone?: string;
  email?: string;
  tags: string[];
  job_id?: number;
}

export interface MergeCandidateImportPayload {
  candidate_id: number;
  current_company?: string;
  years_of_experience?: number;
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
  source: SourceType;
  mode: CrawlMode;
  task_type: string;
  payload: Record<string, unknown>;
}

export interface UpdateTaskPayload {
  task_id: number;
  status: "PENDING" | "RUNNING" | "PAUSED" | "CANCELED" | "SUCCEEDED" | "FAILED";
  retry_count?: number;
  error_code?: string;
  snapshot?: Record<string, unknown>;
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

export type ScreeningRecommendation = "PASS" | "REVIEW" | "REJECT";
export type ScreeningRiskLevel = "LOW" | "MEDIUM" | "HIGH";

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
}

export interface ScreeningResultRecord {
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
  created_at: string;
}

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

export async function listCandidates(stage?: PipelineStage): Promise<CandidateRecord[]> {
  return invoke<CandidateRecord[]>("list_candidates", { stage });
}

export async function createCandidate(input: NewCandidatePayload): Promise<CandidateRecord> {
  return invoke<CandidateRecord>("create_candidate", { input });
}

export async function mergeCandidateImport(input: MergeCandidateImportPayload): Promise<CandidateRecord> {
  return invoke<CandidateRecord>("merge_candidate_import", { input });
}

export async function moveCandidateStage(input: MoveStagePayload): Promise<PipelineEvent> {
  return invoke<PipelineEvent>("move_candidate_stage", { input });
}

export async function listPipelineEvents(candidateId: number): Promise<PipelineEvent[]> {
  return invoke<PipelineEvent[]>("list_pipeline_events", {
    candidate_id: candidateId,
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
}): Promise<BackendAnalysisRecord> {
  return invoke<BackendAnalysisRecord>(
    "run_candidate_analysis",
    {
      input,
    },
  );
}

export async function getScreeningTemplate(job_id?: number): Promise<ScreeningTemplateRecord> {
  return invoke<ScreeningTemplateRecord>("get_screening_template", { job_id });
}

export async function upsertScreeningTemplate(
  input: UpsertScreeningTemplatePayload,
): Promise<ScreeningTemplateRecord> {
  return invoke<ScreeningTemplateRecord>("upsert_screening_template", { input });
}

export async function runResumeScreening(input: {
  candidate_id: number;
  job_id?: number;
}): Promise<ScreeningResultRecord> {
  return invoke<ScreeningResultRecord>("run_resume_screening", { input });
}

export async function listScreeningResults(candidate_id: number): Promise<ScreeningResultRecord[]> {
  return invoke<ScreeningResultRecord[]>("list_screening_results", { candidate_id });
}

export async function listHiringDecisions(candidate_id: number): Promise<HiringDecisionRecord[]> {
  return invoke<HiringDecisionRecord[]>("list_hiring_decisions", { candidate_id });
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

export async function runInterviewEvaluation(
  input: RunInterviewEvaluationPayload,
): Promise<InterviewEvaluationRecord> {
  return invoke<InterviewEvaluationRecord>("run_interview_evaluation", { input });
}

export async function listInterviewEvaluations(candidate_id: number): Promise<InterviewEvaluationRecord[]> {
  return invoke<InterviewEvaluationRecord[]>("list_interview_evaluations", { candidate_id });
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
    candidate_id: candidateId,
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

let sidecarBaseUrl = "http://127.0.0.1:3791";

function sidecarUrl(path: string): string {
  return `${sidecarBaseUrl}${path}`;
}

function setSidecarBaseUrl(baseUrl: string) {
  const trimmed = baseUrl.trim().replace(/\/+$/, "");
  if (trimmed) {
    sidecarBaseUrl = trimmed;
  }
}

function wait(delayMs: number): Promise<void> {
  if (delayMs <= 0) {
    return Promise.resolve();
  }
  return new Promise((resolve) => setTimeout(resolve, delayMs));
}

export async function ensureSidecar(): Promise<SidecarRuntime> {
  const runtime = await invoke<SidecarRuntime>("ensure_sidecar");
  if (!runtime.ok) {
    throw new Error(runtime.message || "Sidecar unavailable");
  }
  setSidecarBaseUrl(runtime.base_url);
  return runtime;
}

async function fetchSidecar(path: string, init: RequestInit, retryOnReconnect: boolean): Promise<Response> {
  const execute = async () => fetch(sidecarUrl(path), init);
  try {
    return await execute();
  } catch (error) {
    if (!retryOnReconnect) {
      throw error;
    }

    await ensureSidecar();
    return execute();
  }
}

export async function checkSidecarHealth(): Promise<{ ok: boolean; service?: string }> {
  const retryDelays = [0, 250, 700];
  let lastError: unknown = null;

  for (let index = 0; index < retryDelays.length; index += 1) {
    if (index > 0) {
      await wait(retryDelays[index] || 0);
    }

    try {
      if (index > 0) {
        await ensureSidecar();
      }
      const response = await fetch(sidecarUrl("/health"), {
        method: "GET",
      });
      if (!response.ok) {
        throw new Error(`Sidecar health failed: ${response.status}`);
      }

      return response.json() as Promise<{ ok: boolean; service?: string }>;
    } catch (error) {
      lastError = error;
    }
  }

  throw lastError instanceof Error ? lastError : new Error("Sidecar health failed");
}

export async function triggerSidecarCrawlJobs(payload: {
  source: Exclude<SourceType, "manual">;
  mode: CrawlMode;
  keyword: string;
  city?: string;
}): Promise<SidecarQueueResult> {
  const response = await fetchSidecar("/v1/crawl/jobs", {
    method: "POST",
    headers: {
      "content-type": "application/json",
    },
    body: JSON.stringify({
      source: payload.source,
      mode: payload.mode,
      params: {
        keyword: payload.keyword,
        city: payload.city,
      },
    }),
  }, true);

  if (!response.ok) {
    throw new Error(`Sidecar crawl failed: ${response.status}`);
  }

  return response.json() as Promise<SidecarQueueResult>;
}

export async function triggerSidecarCrawlCandidates(payload: {
  source: Exclude<SourceType, "manual">;
  mode: CrawlMode;
  jobId: string;
}): Promise<SidecarQueueResult> {
  const response = await fetchSidecar("/v1/crawl/candidates", {
    method: "POST",
    headers: {
      "content-type": "application/json",
    },
    body: JSON.stringify({
      source: payload.source,
      mode: payload.mode,
      params: {
        jobId: payload.jobId,
      },
    }),
  }, true);

  if (!response.ok) {
    throw new Error(`Sidecar candidate crawl failed: ${response.status}`);
  }

  return response.json() as Promise<SidecarQueueResult>;
}

export async function triggerSidecarCrawlResume(payload: {
  source: Exclude<SourceType, "manual">;
  mode: CrawlMode;
  candidateId: string;
}): Promise<SidecarQueueResult> {
  const response = await fetchSidecar("/v1/crawl/resume", {
    method: "POST",
    headers: {
      "content-type": "application/json",
    },
    body: JSON.stringify({
      source: payload.source,
      mode: payload.mode,
      candidateId: payload.candidateId,
    }),
  }, true);

  if (!response.ok) {
    throw new Error(`Sidecar resume crawl failed: ${response.status}`);
  }

  return response.json() as Promise<SidecarQueueResult>;
}
