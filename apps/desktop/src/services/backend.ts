import { invoke } from "@tauri-apps/api/core";
import type {
  CandidateRecord,
  CrawlMode,
  CrawlTaskRecord,
  DashboardMetrics,
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

export interface CrawlTaskPayload {
  source: SourceType;
  mode: CrawlMode;
  task_type: string;
  payload: Record<string, unknown>;
}

export interface UpdateTaskPayload {
  task_id: number;
  status: "PENDING" | "RUNNING" | "SUCCEEDED" | "FAILED";
  retry_count?: number;
  error_code?: string;
  snapshot?: Record<string, unknown>;
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
}

export interface AppHealth {
  ok: boolean;
  dbPath: string;
  dbExists: boolean;
  schemaVersion: number;
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

export async function searchCandidates(query: string): Promise<SearchHit[]> {
  return invoke<SearchHit[]>("search_candidates", { query });
}

export async function loadDashboardMetrics(): Promise<DashboardMetrics> {
  return invoke<DashboardMetrics>("dashboard_metrics");
}

export async function checkSidecarHealth(): Promise<{ ok: boolean; service?: string }> {
  const response = await fetch("http://127.0.0.1:3791/health", {
    method: "GET",
  });
  if (!response.ok) {
    throw new Error(`Sidecar health failed: ${response.status}`);
  }

  return response.json() as Promise<{ ok: boolean; service?: string }>;
}

export async function triggerSidecarCrawlJobs(payload: {
  source: Exclude<SourceType, "manual">;
  mode: CrawlMode;
  keyword: string;
  city?: string;
}): Promise<SidecarQueueResult> {
  const response = await fetch("http://127.0.0.1:3791/v1/crawl/jobs", {
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
  });

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
  const response = await fetch("http://127.0.0.1:3791/v1/crawl/candidates", {
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
  });

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
  const response = await fetch("http://127.0.0.1:3791/v1/crawl/resume", {
    method: "POST",
    headers: {
      "content-type": "application/json",
    },
    body: JSON.stringify({
      source: payload.source,
      mode: payload.mode,
      candidateId: payload.candidateId,
    }),
  });

  if (!response.ok) {
    throw new Error(`Sidecar resume crawl failed: ${response.status}`);
  }

  return response.json() as Promise<SidecarQueueResult>;
}
