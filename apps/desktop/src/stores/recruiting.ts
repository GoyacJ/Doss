import { computed, ref } from "vue";
import { defineStore } from "pinia";
import type {
  CandidateRecord,
  CrawlPlatformSource,
  CrawlMode,
  CrawlTaskPersonRecord,
  CrawlTaskRecord,
  CrawlTaskScheduleType,
  CrawlTaskSource,
  DashboardMetrics,
  JobRecord,
  PipelineStage,
} from "@doss/shared";
import {
  checkSidecarHealth,
  ensureSidecar,
  deleteCrawlTask as deleteCrawlTaskApi,
  createCandidate,
  updateCandidate as updateCandidateApi,
  deleteCandidate as deleteCandidateApi,
  setCandidateQualification as setCandidateQualificationApi,
  createScreeningTemplate as createScreeningTemplateApi,
  createCrawlTask,
  createJob,
  deleteJob as deleteJobApi,
  deleteScreeningTemplate as deleteScreeningTemplateApi,
  getHealth,
  listCandidates,
  listCrawlTasks,
  listCrawlTaskPeople as listCrawlTaskPeopleApi,
  listJobs,
  listScreeningTemplates,
  loadDashboardMetrics,
  mergeCandidateImport,
  moveCandidateStage,
  parseResumeFile,
  getTaskRuntimeSettings,
  searchCandidates,
  testAiProviderSettings,
  triggerSidecarCrawlCandidates,
  triggerSidecarCrawlJobs,
  triggerSidecarCrawlResume,
  setJobScreeningTemplate as setJobScreeningTemplateApi,
  stopJob as stopJobApi,
  upsertTaskRuntimeSettings,
  updateJob as updateJobApi,
  updateScreeningTemplate as updateScreeningTemplateApi,
  updateCrawlTask,
  updateCrawlTaskPeopleSync as updateCrawlTaskPeopleSyncApi,
  upsertCrawlTaskPeople as upsertCrawlTaskPeopleApi,
  type AppHealth,
  type AiProviderId,
  type AiProviderSettings,
  type AiProviderTestResult,
  type ScreeningResultRecord,
  type ScreeningTemplateRecord,
  type UpsertScreeningTemplatePayload,
  type PipelineEvent,
  type InterviewEvaluationRecord,
  type InterviewFeedbackRecord,
  type HiringDecisionRecord,
  type InterviewKitRecord,
  type CreateScreeningTemplatePayload,
  type SearchHit,
  type SetCandidateQualificationPayload,
  type SetJobScreeningTemplatePayload,
  type TaskRuntimeSettings,
  type UpdateCandidatePayload,
  type UpdateJobPayload,
  type UpdateScreeningTemplatePayload,
  finalizeHiringDecision as finalizeHiringDecisionApi,
  generateInterviewKit as generateInterviewKitApi,
  getAiProviderSettings,
  getScreeningTemplate,
  listAnalysis,
  listHiringDecisions,
  listInterviewEvaluations,
  listPipelineEvents,
  listScreeningResults,
  runCandidateAnalysis,
  runInterviewEvaluation as runInterviewEvaluationApi,
  runResumeScreening,
  saveInterviewKit as saveInterviewKitApi,
  submitInterviewFeedback as submitInterviewFeedbackApi,
  upsertResume,
  upsertScreeningTemplate,
  upsertAiProviderSettings,
} from "../services/backend";
import { extractCandidateImportItems, extractJobImportItems } from "../lib/crawl-import";
import {
  createAnalysisContextModule,
  mapBackendAnalysisRecord,
} from "./recruiting/analysis-context";
import { createCandidateImportModule } from "./recruiting/candidate-import";
import { createTaskOrchestrator } from "./recruiting/task-orchestrator";
import { CRAWL_PLATFORM_SOURCES } from "./recruiting/types";
import type {
  CandidateImportConflict,
  CrawlCandidatesTaskPayload,
  CandidateImportQualityReport,
  CandidateImportSource,
  CrawlTaskSource as LocalCrawlTaskSource,
  ConflictResolutionAction,
  UiAnalysisRecord,
} from "./recruiting/types";

export type {
  CandidateImportConflict,
  CandidateImportQualityReport,
} from "./recruiting/types";

const DEFAULT_CANDIDATE_TASK_PAYLOAD: CrawlCandidatesTaskPayload = {
  localJobId: 0,
  localJobTitle: "",
  localJobCity: "",
  batchSize: 50,
  scheduleType: "ONCE",
  scheduleTime: "09:30",
  scheduleDay: 1,
  retryCount: 1,
  retryBackoffMs: 450,
  autoSyncToCandidates: true,
};

function wait(delayMs: number): Promise<void> {
  if (delayMs <= 0) {
    return Promise.resolve();
  }
  return new Promise((resolve) => setTimeout(resolve, delayMs));
}

function asNumber(value: unknown, fallback: number): number {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string") {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) {
      return parsed;
    }
  }
  return fallback;
}

function asString(value: unknown, fallback = ""): string {
  if (typeof value === "string") {
    return value;
  }
  return fallback;
}

function asBoolean(value: unknown, fallback: boolean): boolean {
  if (typeof value === "boolean") {
    return value;
  }
  return fallback;
}

function normalizeScheduleType(value: unknown): CrawlTaskScheduleType {
  if (typeof value !== "string") {
    return "ONCE";
  }
  const normalized = value.trim().toUpperCase();
  if (normalized === "DAILY" || normalized === "MONTHLY") {
    return normalized;
  }
  return "ONCE";
}

function normalizeScheduleTime(value: unknown, fallback = "09:30"): string {
  if (typeof value !== "string") {
    return fallback;
  }
  const trimmed = value.trim();
  if (!/^\d{2}:\d{2}$/.test(trimmed)) {
    return fallback;
  }
  const [hoursText, minutesText] = trimmed.split(":");
  const hours = Number(hoursText);
  const minutes = Number(minutesText);
  if (!Number.isInteger(hours) || !Number.isInteger(minutes)) {
    return fallback;
  }
  if (hours < 0 || hours > 23 || minutes < 0 || minutes > 59) {
    return fallback;
  }
  return `${hoursText}:${minutesText}`;
}

function normalizeScheduleDay(value: unknown, fallback = 1): number {
  const day = Math.trunc(asNumber(value, fallback));
  if (!Number.isFinite(day)) {
    return fallback;
  }
  return Math.min(31, Math.max(1, day));
}

function parseTimeParts(scheduleTime: string): { hours: number; minutes: number } {
  const [hoursText, minutesText] = scheduleTime.split(":");
  return {
    hours: Number(hoursText),
    minutes: Number(minutesText),
  };
}

function lastDayOfMonth(year: number, monthIndex: number): number {
  return new Date(year, monthIndex + 1, 0).getDate();
}

function computeNextRunAt(
  scheduleType: CrawlTaskScheduleType,
  scheduleTime: string,
  scheduleDay: number,
  now = new Date(),
): string | null {
  if (scheduleType === "ONCE") {
    return now.toISOString();
  }

  const { hours, minutes } = parseTimeParts(scheduleTime);
  if (scheduleType === "DAILY") {
    const candidate = new Date(now);
    candidate.setHours(hours, minutes, 0, 0);
    if (candidate.getTime() <= now.getTime()) {
      candidate.setDate(candidate.getDate() + 1);
    }
    return candidate.toISOString();
  }

  const candidate = new Date(now);
  candidate.setHours(hours, minutes, 0, 0);
  const targetDay = Math.min(scheduleDay, lastDayOfMonth(candidate.getFullYear(), candidate.getMonth()));
  candidate.setDate(targetDay);
  if (candidate.getTime() <= now.getTime()) {
    candidate.setMonth(candidate.getMonth() + 1, 1);
    const nextTargetDay = Math.min(scheduleDay, lastDayOfMonth(candidate.getFullYear(), candidate.getMonth()));
    candidate.setDate(nextTargetDay);
    candidate.setHours(hours, minutes, 0, 0);
  }
  return candidate.toISOString();
}

function parseCandidatesTaskPayload(task: CrawlTaskRecord): CrawlCandidatesTaskPayload {
  const payload = task.payload ?? {};
  const legacyIntervalSeconds = Math.max(1, Math.trunc(asNumber(
    payload.crawlIntervalSeconds,
    300,
  )));
  const fallbackScheduleType = legacyIntervalSeconds > 0 ? "DAILY" : "ONCE";
  return {
    localJobId: Math.max(0, Math.trunc(asNumber(payload.localJobId, DEFAULT_CANDIDATE_TASK_PAYLOAD.localJobId))),
    localJobTitle: asString(payload.localJobTitle, DEFAULT_CANDIDATE_TASK_PAYLOAD.localJobTitle).trim(),
    localJobCity: asString(payload.localJobCity, DEFAULT_CANDIDATE_TASK_PAYLOAD.localJobCity).trim(),
    batchSize: Math.max(1, Math.trunc(asNumber(payload.batchSize, DEFAULT_CANDIDATE_TASK_PAYLOAD.batchSize))),
    scheduleType: normalizeScheduleType(task.schedule_type ?? payload.scheduleType ?? fallbackScheduleType),
    scheduleTime: normalizeScheduleTime(task.schedule_time ?? payload.scheduleTime ?? DEFAULT_CANDIDATE_TASK_PAYLOAD.scheduleTime),
    scheduleDay: normalizeScheduleDay(task.schedule_day ?? payload.scheduleDay ?? DEFAULT_CANDIDATE_TASK_PAYLOAD.scheduleDay),
    retryCount: Math.max(0, Math.trunc(asNumber(payload.retryCount, DEFAULT_CANDIDATE_TASK_PAYLOAD.retryCount))),
    retryBackoffMs: Math.max(100, Math.trunc(asNumber(payload.retryBackoffMs, DEFAULT_CANDIDATE_TASK_PAYLOAD.retryBackoffMs))),
    autoSyncToCandidates: asBoolean(payload.autoSyncToCandidates, DEFAULT_CANDIDATE_TASK_PAYLOAD.autoSyncToCandidates),
  };
}

export const useRecruitingStore = defineStore("recruiting", () => {
  const SIDECAR_HEALTH_REFRESH_INTERVAL_MS = 15_000;
  const loading = ref(false);
  const lastError = ref<string | null>(null);

  const jobs = ref<JobRecord[]>([]);
  const candidates = ref<CandidateRecord[]>([]);
  const tasks = ref<CrawlTaskRecord[]>([]);
  const taskPeople = ref<Record<number, CrawlTaskPersonRecord[]>>({});
  const metrics = ref<DashboardMetrics | null>(null);
  const health = ref<AppHealth | null>(null);
  const sidecarHealthy = ref<boolean | null>(null);
  const sidecarError = ref<string | null>(null);

  const analyses = ref<Record<number, UiAnalysisRecord[]>>({});
  const screeningResults = ref<Record<number, ScreeningResultRecord[]>>({});
  const interviewKits = ref<Record<number, InterviewKitRecord | null>>({});
  const interviewFeedback = ref<Record<number, InterviewFeedbackRecord[]>>({});
  const interviewEvaluations = ref<Record<number, InterviewEvaluationRecord[]>>({});
  const hiringDecisions = ref<Record<number, HiringDecisionRecord[]>>({});
  const pipelineEvents = ref<Record<number, PipelineEvent[]>>({});
  const searchResults = ref<SearchHit[]>([]);
  const aiSettings = ref<AiProviderSettings | null>(null);
  const taskSettings = ref<TaskRuntimeSettings>({
    auto_batch_concurrency: 2,
    auto_retry_count: 1,
    auto_retry_backoff_ms: 450,
  });
  const candidateImportConflicts = ref<CandidateImportConflict[]>([]);
  const lastCandidateImportReport = ref<CandidateImportQualityReport | null>(null);
  const activeScreeningTemplate = ref<ScreeningTemplateRecord | null>(null);
  const screeningTemplates = ref<ScreeningTemplateRecord[]>([]);

  const hasBootstrapped = ref(false);
  const stageSummary = computed(() => metrics.value?.stage_stats ?? []);
  const taskLoopTimers = new Map<number, ReturnType<typeof setTimeout>>();
  const taskLoopLocks = new Set<number>();
  let sidecarHealthTimer: ReturnType<typeof setInterval> | null = null;
  let sidecarHealthRefreshing = false;

  function setError(error: unknown) {
    if (error instanceof Error) {
      lastError.value = error.message;
      return;
    }
    lastError.value = "Unknown error";
  }

  function resolveErrorMessage(error: unknown, fallback: string): string {
    if (error instanceof Error && error.message.trim()) {
      return error.message;
    }
    if (typeof error === "string" && error.trim()) {
      return error;
    }
    return fallback;
  }

  async function refreshSidecarHealth(options: { ensure?: boolean } = {}) {
    if (sidecarHealthRefreshing) {
      return;
    }

    sidecarHealthRefreshing = true;
    try {
      if (options.ensure !== false) {
        await ensureSidecar();
      }

      const sidecar = await checkSidecarHealth();
      sidecarHealthy.value = sidecar.ok;
      sidecarError.value = sidecar.ok ? null : "sidecar_health_not_ok";
    } catch (error) {
      sidecarHealthy.value = false;
      sidecarError.value = resolveErrorMessage(error, "sidecar_unavailable");
    } finally {
      sidecarHealthRefreshing = false;
    }
  }

  function startSidecarHealthPolling(intervalMs = SIDECAR_HEALTH_REFRESH_INTERVAL_MS) {
    if (sidecarHealthTimer) {
      return;
    }
    sidecarHealthTimer = setInterval(() => {
      void refreshSidecarHealth();
    }, Math.max(1_000, Math.trunc(intervalMs)));
  }

  function stopSidecarHealthPolling() {
    if (!sidecarHealthTimer) {
      return;
    }
    clearInterval(sidecarHealthTimer);
    sidecarHealthTimer = null;
  }

  async function bootstrap() {
    if (loading.value) {
      return;
    }

    loading.value = true;
    lastError.value = null;
    try {
      const [jobsData, candidatesData, tasksData, metricsData, healthData] = await Promise.all([
        listJobs(),
        listCandidates(),
        listCrawlTasks(),
        loadDashboardMetrics(),
        getHealth(),
      ]);
      jobs.value = jobsData;
      candidates.value = candidatesData;
      tasks.value = tasksData;
      syncTaskLoopState();
      metrics.value = metricsData;
      health.value = healthData;

      await refreshSidecarHealth();

      try {
        aiSettings.value = await getAiProviderSettings();
      } catch {
        aiSettings.value = null;
      }

      try {
        taskSettings.value = await getTaskRuntimeSettings();
      } catch {
        taskSettings.value = {
          auto_batch_concurrency: 2,
          auto_retry_count: 1,
          auto_retry_backoff_ms: 450,
        };
      }

      hasBootstrapped.value = true;
    } catch (error) {
      setError(error);
      throw error;
    } finally {
      loading.value = false;
    }
  }

  async function refreshMetrics() {
    metrics.value = await loadDashboardMetrics();
  }

  function clearTaskLoopTimer(taskId: number) {
    const timer = taskLoopTimers.get(taskId);
    if (timer) {
      clearTimeout(timer);
      taskLoopTimers.delete(taskId);
    }
  }

  function scheduleTaskLoop(taskId: number, delayMs: number) {
    clearTaskLoopTimer(taskId);
    const timer = setTimeout(() => {
      runCrawlTaskOnce(taskId).catch((error) => {
        setError(error);
      });
    }, Math.max(0, delayMs));
    taskLoopTimers.set(taskId, timer);
  }

  function resolveTaskNextRunDelayMs(task: CrawlTaskRecord): number {
    if (!task.next_run_at) {
      return 0;
    }
    const runAtMs = Date.parse(task.next_run_at);
    if (!Number.isFinite(runAtMs)) {
      return 0;
    }
    const now = Date.now();
    return Math.max(0, runAtMs - now);
  }

  function findTaskById(taskId: number): CrawlTaskRecord | undefined {
    return tasks.value.find((item) => item.id === taskId);
  }

  function resolveTaskPlatforms(source: CrawlTaskSource): CrawlPlatformSource[] {
    if (source === "all") {
      return [...CRAWL_PLATFORM_SOURCES];
    }
    return [source];
  }

  async function ensureTaskPeopleLoaded(taskId: number) {
    if (taskPeople.value[taskId]) {
      return taskPeople.value[taskId];
    }
    const people = await listCrawlTaskPeopleApi(taskId);
    taskPeople.value[taskId] = people;
    return people;
  }

  async function syncTaskPeople(taskId: number) {
    const task = findTaskById(taskId);
    if (!task) {
      return [];
    }
    const taskPayload = parseCandidatesTaskPayload(task);
    const allPeople = await ensureTaskPeopleLoaded(taskId);
    const pendingPeople = allPeople.filter((person) => person.sync_status === "UNSYNCED");
    if (pendingPeople.length === 0) {
      return allPeople;
    }

    const updatePayload: Array<{
      person_id: number;
      sync_status: "UNSYNCED" | "SYNCED" | "FAILED";
      sync_error_code?: string;
      sync_error_message?: string;
      candidate_id?: number;
    }> = [];

    for (const person of pendingPeople) {
      try {
        const result = await candidateImportModule.importSingleCandidateItem({
          item: {
            external_id: person.external_id ?? undefined,
            name: person.name,
            current_company: person.current_company ?? undefined,
            years_of_experience: person.years_of_experience,
            tags: [],
            phone: undefined,
            email: undefined,
          },
          source: person.source as CandidateImportSource,
          mode: task.mode,
          localJobId: taskPayload.localJobId,
        });

        updatePayload.push({
          person_id: person.id,
          sync_status: result.status,
          sync_error_code: result.status === "FAILED" ? "candidate_sync_failed" : undefined,
          sync_error_message: result.reason,
          candidate_id: result.candidateId,
        });
      } catch (error) {
        updatePayload.push({
          person_id: person.id,
          sync_status: "FAILED",
          sync_error_code: "candidate_sync_failed",
          sync_error_message: error instanceof Error ? error.message : "candidate_sync_failed",
        });
      }
    }

    const updated = await updateCrawlTaskPeopleSyncApi({
      task_id: taskId,
      updates: updatePayload,
    });
    taskPeople.value[taskId] = updated;
    await refreshMetrics();
    return updated;
  }

  async function runCrawlTaskCycle(task: CrawlTaskRecord): Promise<{
    fetchedPeople: number;
    syncedPeople: number;
    failedPeople: number;
    platformSummaries: Array<{
      source: CrawlPlatformSource;
      jobId?: string;
      fetched: number;
      skipped: boolean;
      reason?: string;
    }>;
  }> {
    if (task.task_type !== "crawl_candidates") {
      throw new Error("unsupported_task_type");
    }

    const payload = parseCandidatesTaskPayload(task);
    if (!payload.localJobTitle.trim()) {
      throw new Error("task_local_job_title_required");
    }

    const platformSummaries: Array<{
      source: CrawlPlatformSource;
      jobId?: string;
      fetched: number;
      skipped: boolean;
      reason?: string;
    }> = [];
    const mergedPeople: Array<{
      source: CrawlPlatformSource;
      external_id?: string;
      name: string;
      current_company?: string;
      years_of_experience: number;
    }> = [];

    for (const source of resolveTaskPlatforms(task.source as CrawlTaskSource)) {
      const jobsResult = await triggerSidecarCrawlJobs({
        source,
        mode: task.mode,
        keyword: payload.localJobTitle,
        city: payload.localJobCity || undefined,
      });
      if (jobsResult.status === "FAILED") {
        throw new Error(jobsResult.error || `crawl_jobs_failed_${source}`);
      }

      const jobs = extractJobImportItems(jobsResult);
      const selectedJob = jobs[0];
      if (!selectedJob?.external_id) {
        platformSummaries.push({
          source,
          fetched: 0,
          skipped: true,
          reason: "job_not_found",
        });
        continue;
      }

      const candidateResult = await triggerSidecarCrawlCandidates({
        source,
        mode: task.mode,
        jobId: selectedJob.external_id,
      });
      if (candidateResult.status === "FAILED") {
        throw new Error(candidateResult.error || `crawl_candidates_failed_${source}`);
      }

      const currentCandidates = extractCandidateImportItems(candidateResult);
      platformSummaries.push({
        source,
        jobId: selectedJob.external_id,
        fetched: currentCandidates.length,
        skipped: false,
      });

      for (const item of currentCandidates) {
        if (mergedPeople.length >= payload.batchSize) {
          break;
        }
        mergedPeople.push({
          source,
          external_id: item.external_id,
          name: item.name,
          current_company: item.current_company,
          years_of_experience: item.years_of_experience,
        });
      }

      if (mergedPeople.length >= payload.batchSize) {
        break;
      }
    }

    const upserted = await upsertCrawlTaskPeopleApi({
      task_id: task.id,
      people: mergedPeople.map((person) => ({
        source: person.source,
        external_id: person.external_id,
        name: person.name,
        current_company: person.current_company,
        years_of_experience: person.years_of_experience,
        sync_status: "UNSYNCED",
      })),
    });
    taskPeople.value[task.id] = upserted;

    let syncedPeople = 0;
    let failedPeople = 0;
    if (payload.autoSyncToCandidates) {
      const afterSync = await syncTaskPeople(task.id);
      syncedPeople = afterSync.filter((item) => item.sync_status === "SYNCED").length;
      failedPeople = afterSync.filter((item) => item.sync_status === "FAILED").length;
    }

    return {
      fetchedPeople: mergedPeople.length,
      syncedPeople,
      failedPeople,
      platformSummaries,
    };
  }

  async function runCrawlTaskOnce(taskId: number) {
    if (taskLoopLocks.has(taskId)) {
      return;
    }
    const task = findTaskById(taskId);
    if (!task || task.status !== "RUNNING") {
      clearTaskLoopTimer(taskId);
      return;
    }

    const pendingDelayMs = resolveTaskNextRunDelayMs(task);
    if (pendingDelayMs > 250) {
      scheduleTaskLoop(taskId, pendingDelayMs);
      return;
    }

    taskLoopLocks.add(taskId);
    try {
      const payload = parseCandidatesTaskPayload(task);
      let attempts = 0;
      let lastError: unknown = null;
      while (attempts <= payload.retryCount) {
        try {
          const result = await runCrawlTaskCycle(task);
          const nextRunAt = computeNextRunAt(
            payload.scheduleType,
            payload.scheduleTime ?? DEFAULT_CANDIDATE_TASK_PAYLOAD.scheduleTime ?? "09:30",
            payload.scheduleDay ?? DEFAULT_CANDIDATE_TASK_PAYLOAD.scheduleDay ?? 1,
          );
          const nextStatus = payload.scheduleType === "ONCE" ? "SUCCEEDED" : "RUNNING";
          await updateCrawlTask({
            task_id: taskId,
            status: nextStatus,
            error_code: undefined,
            schedule_type: payload.scheduleType,
            schedule_time: payload.scheduleTime,
            schedule_day: payload.scheduleDay,
            next_run_at: payload.scheduleType === "ONCE" ? undefined : nextRunAt ?? undefined,
            snapshot: {
              ...(task.snapshot ?? {}),
              lastRunAt: new Date().toISOString(),
              fetchedPeople: result.fetchedPeople,
              syncedPeople: result.syncedPeople,
              failedPeople: result.failedPeople,
              platformSummaries: result.platformSummaries,
              scheduleType: payload.scheduleType,
              scheduleTime: payload.scheduleTime,
              scheduleDay: payload.scheduleDay,
              nextRunAt: nextRunAt ?? null,
            },
          });
          await refreshTasks();
          const latest = findTaskById(taskId);
          if (latest?.status === "RUNNING") {
            scheduleTaskLoop(taskId, resolveTaskNextRunDelayMs(latest));
          }
          return;
        } catch (error) {
          lastError = error;
          if (attempts >= payload.retryCount) {
            break;
          }
          await wait(payload.retryBackoffMs * (attempts + 1));
        }
        attempts += 1;
      }

      await updateCrawlTask({
        task_id: taskId,
        status: "FAILED",
        error_code: lastError instanceof Error ? lastError.message : "crawl_task_cycle_failed",
        snapshot: {
          ...(task.snapshot ?? {}),
          lastFailedAt: new Date().toISOString(),
        },
      });
      clearTaskLoopTimer(taskId);
      await refreshTasks();
    } finally {
      taskLoopLocks.delete(taskId);
    }
  }

  function syncTaskLoopState() {
    const runningIds = new Set(
      tasks.value
        .filter((task) => task.status === "RUNNING" && task.task_type === "crawl_candidates")
        .map((task) => task.id),
    );

    for (const [taskId] of taskLoopTimers) {
      if (!runningIds.has(taskId)) {
        clearTaskLoopTimer(taskId);
      }
    }

    for (const taskId of runningIds) {
      if (!taskLoopTimers.has(taskId)) {
        const task = findTaskById(taskId);
        if (!task) {
          continue;
        }
        scheduleTaskLoop(taskId, resolveTaskNextRunDelayMs(task));
      }
    }
  }

  async function refreshTasks() {
    tasks.value = await listCrawlTasks();
    syncTaskLoopState();
  }

  function upsertJobInList(job: JobRecord) {
    const index = jobs.value.findIndex((item) => item.id === job.id);
    if (index >= 0) {
      jobs.value.splice(index, 1, job);
      return;
    }
    jobs.value.unshift(job);
  }

  function upsertCandidateInList(candidate: CandidateRecord) {
    const index = candidates.value.findIndex((item) => item.id === candidate.id);
    if (index >= 0) {
      candidates.value.splice(index, 1, candidate);
      return;
    }
    candidates.value.unshift(candidate);
  }

  async function addJob(payload: {
    title: string;
    company: string;
    city?: string;
    salary_k?: string;
    description?: string;
  }) {
    const job = await createJob({
      source: "manual",
      ...payload,
    });
    upsertJobInList(job);
    await refreshMetrics();
    return job;
  }

  async function updateJob(payload: UpdateJobPayload) {
    const job = await updateJobApi(payload);
    upsertJobInList(job);
    await refreshMetrics();
    return job;
  }

  async function stopJob(jobId: number) {
    const job = await stopJobApi(jobId);
    upsertJobInList(job);
    await refreshMetrics();
    return job;
  }

  async function deleteJob(jobId: number) {
    await deleteJobApi(jobId);
    jobs.value = jobs.value.filter((item) => item.id !== jobId);
    await refreshMetrics();
    return true;
  }

  async function addCandidate(payload: {
    name: string;
    current_company?: string;
    score?: number;
    age?: number;
    gender?: "male" | "female" | "other";
    years_of_experience: number;
    address?: string;
    phone?: string;
    email?: string;
    tags: string[];
    job_id?: number;
  }) {
    const candidate = await createCandidate({
      source: "manual",
      ...payload,
    });
    upsertCandidateInList(candidate);
    await refreshMetrics();
    return candidate;
  }

  async function updateCandidate(payload: UpdateCandidatePayload) {
    const candidate = await updateCandidateApi(payload);
    upsertCandidateInList(candidate);
    await refreshMetrics();
    return candidate;
  }

  async function deleteCandidate(candidateId: number) {
    await deleteCandidateApi(candidateId);
    candidates.value = candidates.value.filter((item) => item.id !== candidateId);
    delete analyses.value[candidateId];
    delete screeningResults.value[candidateId];
    delete interviewKits.value[candidateId];
    delete interviewFeedback.value[candidateId];
    delete interviewEvaluations.value[candidateId];
    delete hiringDecisions.value[candidateId];
    delete pipelineEvents.value[candidateId];
    searchResults.value = searchResults.value.filter((item) => item.candidate_id !== candidateId);
    await refreshMetrics();
    return true;
  }

  async function setCandidateQualification(payload: SetCandidateQualificationPayload) {
    const candidate = await setCandidateQualificationApi(payload);
    upsertCandidateInList(candidate);
    pipelineEvents.value[payload.candidate_id] = await listPipelineEvents(payload.candidate_id);
    await refreshMetrics();
    return candidate;
  }

  async function moveStage(payload: {
    candidate_id: number;
    to_stage: PipelineStage;
    note?: string;
    job_id?: number;
  }) {
    await moveCandidateStage(payload);
    candidates.value = await listCandidates();
    pipelineEvents.value[payload.candidate_id] = await listPipelineEvents(payload.candidate_id);
    await refreshMetrics();
  }

  async function addCrawlTask(payload: {
    source: CandidateImportSource;
    mode: CrawlMode;
    task_type: string;
    payload: Record<string, unknown>;
  }) {
    const task = await createCrawlTask(payload);
    tasks.value.unshift(task);
    await refreshMetrics();
    return task;
  }

  async function createCandidatesTask(payload: {
    source: LocalCrawlTaskSource;
    mode: CrawlMode;
    localJobId: number;
    batchSize: number;
    scheduleType: CrawlTaskScheduleType;
    scheduleTime?: string;
    scheduleDay?: number;
    retryCount: number;
    retryBackoffMs: number;
    autoSyncToCandidates: boolean;
  }) {
    const localJob = jobs.value.find((job) => job.id === payload.localJobId);
    if (!localJob || localJob.status === "STOPPED") {
      throw new Error("local_active_job_required");
    }

    const scheduleType = normalizeScheduleType(payload.scheduleType);
    const scheduleTime = normalizeScheduleTime(payload.scheduleTime, DEFAULT_CANDIDATE_TASK_PAYLOAD.scheduleTime);
    const scheduleDay = normalizeScheduleDay(payload.scheduleDay, DEFAULT_CANDIDATE_TASK_PAYLOAD.scheduleDay);
    const nextRunAt = computeNextRunAt(scheduleType, scheduleTime, scheduleDay);

    const task = await createCrawlTask({
      source: payload.source,
      mode: payload.mode,
      task_type: "crawl_candidates",
      schedule_type: scheduleType,
      schedule_time: scheduleTime,
      schedule_day: scheduleDay,
      next_run_at: nextRunAt ?? undefined,
      payload: {
        localJobId: localJob.id,
        localJobTitle: localJob.title,
        localJobCity: localJob.city ?? "",
        batchSize: payload.batchSize,
        scheduleType,
        scheduleTime,
        scheduleDay,
        retryCount: payload.retryCount,
        retryBackoffMs: payload.retryBackoffMs,
        autoSyncToCandidates: payload.autoSyncToCandidates,
      },
    });

    tasks.value.unshift(task);
    await refreshMetrics();
    return task;
  }

  const analysisContext = createAnalysisContextModule({
    analyses,
    screeningResults,
    interviewKits,
    interviewFeedback,
    interviewEvaluations,
    hiringDecisions,
    pipelineEvents,
    mapAnalysis: mapBackendAnalysisRecord,
    runCandidateAnalysis,
    listAnalysis,
    listHiringDecisions,
    listInterviewEvaluations,
    listPipelineEvents,
    listScreeningResults,
    runResumeScreening,
    upsertResume,
    refreshMetrics,
    parseResumeFile,
    generateInterviewKit: generateInterviewKitApi,
    saveInterviewKit: saveInterviewKitApi,
    submitInterviewFeedback: submitInterviewFeedbackApi,
    runInterviewEvaluation: runInterviewEvaluationApi,
  });

  const candidateImportModule = createCandidateImportModule({
    jobs,
    candidates,
    candidateImportConflicts,
    createJob,
    createCandidate,
    mergeCandidateImport,
  });

  const taskOrchestrator = createTaskOrchestrator({
    candidates,
    taskSettings,
    candidateImportConflicts,
    lastCandidateImportReport,
    addCrawlTask,
    updateCrawlTask,
    refreshTasks,
    refreshMetrics,
    importJobsFromSidecarResult: candidateImportModule.importJobsFromSidecarResult,
    importCandidatesFromSidecarResult: candidateImportModule.importCandidatesFromSidecarResult,
    triggerSidecarCrawlJobs,
    triggerSidecarCrawlCandidates,
    triggerSidecarCrawlResume,
    saveResume: analysisContext.saveResume,
    loadCandidateContext: analysisContext.loadCandidateContext,
    analyzeCandidate: analysisContext.analyzeCandidate,
    mergeCandidateImport,
    createCandidate,
  });

  async function search(query: string): Promise<{ ok: boolean; error?: string }> {
    if (!query.trim()) {
      searchResults.value = [];
      return { ok: true };
    }
    try {
      searchResults.value = await searchCandidates(query);
      return { ok: true };
    } catch (error) {
      searchResults.value = [];
      return {
        ok: false,
        error: error instanceof Error ? error.message : "candidate_search_failed",
      };
    }
  }

  async function loadAiSettings() {
    aiSettings.value = await getAiProviderSettings();
    return aiSettings.value;
  }

  async function loadScreeningTemplate(jobId?: number) {
    activeScreeningTemplate.value = await getScreeningTemplate(jobId);
    return activeScreeningTemplate.value;
  }

  async function saveScreeningTemplate(input: UpsertScreeningTemplatePayload) {
    activeScreeningTemplate.value = await upsertScreeningTemplate(input);
    return activeScreeningTemplate.value;
  }

  async function loadScreeningTemplates() {
    screeningTemplates.value = await listScreeningTemplates();
    return screeningTemplates.value;
  }

  async function createScreeningTemplate(input: CreateScreeningTemplatePayload) {
    const created = await createScreeningTemplateApi(input);
    screeningTemplates.value = [created, ...screeningTemplates.value.filter((item) => item.id !== created.id)];
    return created;
  }

  async function updateScreeningTemplate(input: UpdateScreeningTemplatePayload) {
    const updated = await updateScreeningTemplateApi(input);
    const index = screeningTemplates.value.findIndex((item) => item.id === updated.id);
    if (index >= 0) {
      screeningTemplates.value.splice(index, 1, updated);
    } else {
      screeningTemplates.value.unshift(updated);
    }
    if (activeScreeningTemplate.value?.id === updated.id) {
      activeScreeningTemplate.value = updated;
    }
    jobs.value = jobs.value.map((job) =>
      job.screening_template_id === updated.id
        ? {
            ...job,
            screening_template_name: updated.name,
          }
        : job,
    );
    return updated;
  }

  async function deleteScreeningTemplate(templateId: number) {
    screeningTemplates.value = await deleteScreeningTemplateApi(templateId);
    if (
      activeScreeningTemplate.value
      && !screeningTemplates.value.some((item) => item.id === activeScreeningTemplate.value?.id)
    ) {
      activeScreeningTemplate.value = null;
    }
    jobs.value = jobs.value.map((job) =>
      job.screening_template_id === templateId
        ? {
            ...job,
            screening_template_id: null,
            screening_template_name: null,
          }
        : job,
    );
    return screeningTemplates.value;
  }

  async function setJobScreeningTemplate(input: SetJobScreeningTemplatePayload) {
    const job = await setJobScreeningTemplateApi(input);
    upsertJobInList(job);
    return job;
  }

  async function saveAiSettings(payload: {
    provider: AiProviderId;
    model?: string;
    base_url?: string;
    temperature?: number;
    max_tokens?: number;
    timeout_secs?: number;
    retry_count?: number;
    api_key?: string;
  }) {
    aiSettings.value = await upsertAiProviderSettings(payload);
    return aiSettings.value;
  }

  async function testAiSettings(payload: {
    provider: AiProviderId;
    model?: string;
    base_url?: string;
    temperature?: number;
    max_tokens?: number;
    timeout_secs?: number;
    retry_count?: number;
    api_key?: string;
  }): Promise<AiProviderTestResult> {
    return testAiProviderSettings(payload);
  }

  async function finalizeHiringDecision(payload: {
    candidate_id: number;
    job_id?: number;
    final_decision: "HIRE" | "NO_HIRE";
    reason_code: string;
    note?: string;
  }) {
    const decision = await finalizeHiringDecisionApi(payload);
    const existing = hiringDecisions.value[payload.candidate_id] ?? [];
    hiringDecisions.value[payload.candidate_id] = [decision, ...existing];
    candidates.value = await listCandidates();
    pipelineEvents.value[payload.candidate_id] = await listPipelineEvents(payload.candidate_id);
    await refreshMetrics();
    return decision;
  }

  async function loadTaskSettings() {
    taskSettings.value = await getTaskRuntimeSettings();
    return taskSettings.value;
  }

  async function saveTaskSettings(payload: {
    auto_batch_concurrency?: number;
    auto_retry_count?: number;
    auto_retry_backoff_ms?: number;
  }) {
    taskSettings.value = await upsertTaskRuntimeSettings(payload);
    return taskSettings.value;
  }

  async function loadTaskPeople(taskId: number) {
    const people = await listCrawlTaskPeopleApi(taskId);
    taskPeople.value[taskId] = people;
    return people;
  }

  async function toggleTaskRunState(taskId: number) {
    const task = findTaskById(taskId);
    if (!task) {
      throw new Error("crawl_task_not_found");
    }

    if (task.status === "RUNNING") {
      await updateCrawlTask({
        task_id: taskId,
        status: "PENDING",
      });
      clearTaskLoopTimer(taskId);
      await refreshTasks();
      return;
    }

    const payload = parseCandidatesTaskPayload(task);
    const nextRunAt = task.next_run_at
      ?? computeNextRunAt(
        payload.scheduleType,
        payload.scheduleTime ?? DEFAULT_CANDIDATE_TASK_PAYLOAD.scheduleTime ?? "09:30",
        payload.scheduleDay ?? DEFAULT_CANDIDATE_TASK_PAYLOAD.scheduleDay ?? 1,
      )
      ?? undefined;
    await updateCrawlTask({
      task_id: taskId,
      status: "RUNNING",
      schedule_type: payload.scheduleType,
      schedule_time: payload.scheduleTime,
      schedule_day: payload.scheduleDay,
      next_run_at: nextRunAt,
    });
    await refreshTasks();
    await runCrawlTaskOnce(taskId);
  }

  async function deleteTask(taskId: number) {
    const task = findTaskById(taskId);
    if (!task) {
      throw new Error("crawl_task_not_found");
    }

    if (task.status === "RUNNING") {
      await updateCrawlTask({
        task_id: taskId,
        status: "PAUSED",
      });
    }
    clearTaskLoopTimer(taskId);
    taskLoopLocks.delete(taskId);

    await deleteCrawlTaskApi(taskId);
    tasks.value = tasks.value.filter((item) => item.id !== taskId);
    delete taskPeople.value[taskId];
    await refreshMetrics();
  }

  async function pauseTask(taskId: number) {
    clearTaskLoopTimer(taskId);
    await updateCrawlTask({
      task_id: taskId,
      status: "PAUSED",
    });
    await refreshTasks();
  }

  async function resumeTask(taskId: number) {
    await updateCrawlTask({
      task_id: taskId,
      status: "PENDING",
    });
    await refreshTasks();
  }

  async function cancelTask(taskId: number) {
    clearTaskLoopTimer(taskId);
    await updateCrawlTask({
      task_id: taskId,
      status: "CANCELED",
    });
    await refreshTasks();
  }

  return {
    loading,
    lastError,
    jobs,
    candidates,
    tasks,
    taskPeople,
    metrics,
    health,
    sidecarHealthy,
    sidecarError,
    analyses,
    screeningResults,
    interviewKits,
    interviewFeedback,
    interviewEvaluations,
    hiringDecisions,
    pipelineEvents,
    searchResults,
    aiSettings,
    activeScreeningTemplate,
    screeningTemplates,
    taskSettings,
    candidateImportConflicts,
    lastCandidateImportReport,
    hasBootstrapped,
    stageSummary,
    bootstrap,
    refreshSidecarHealth,
    startSidecarHealthPolling,
    stopSidecarHealthPolling,
    refreshMetrics,
    refreshTasks,
    addJob,
    updateJob,
    stopJob,
    deleteJob,
    addCandidate,
    updateCandidate,
    deleteCandidate,
    setCandidateQualification,
    moveStage,
    saveResume: analysisContext.saveResume,
    analyzeCandidate: analysisContext.analyzeCandidate,
    loadCandidateContext: analysisContext.loadCandidateContext,
    addCrawlTask,
    createCandidatesTask,
    runSidecarJobCrawl: taskOrchestrator.runSidecarJobCrawl,
    runSidecarCandidateCrawl: taskOrchestrator.runSidecarCandidateCrawl,
    runSidecarResumeCrawl: taskOrchestrator.runSidecarResumeCrawl,
    resolveCandidateImportConflict: async (payload: {
      conflictId: string;
      action: ConflictResolutionAction;
    }) => taskOrchestrator.resolveCandidateImportConflict(payload),
    search,
    importResumeFileAndAnalyze: analysisContext.importResumeFileAndAnalyze,
    loadScreeningTemplate,
    saveScreeningTemplate,
    loadScreeningTemplates,
    createScreeningTemplate,
    updateScreeningTemplate,
    deleteScreeningTemplate,
    setJobScreeningTemplate,
    runScreening: analysisContext.runScreening,
    generateInterviewKit: analysisContext.generateInterviewKit,
    saveInterviewKit: analysisContext.saveInterviewKit,
    submitInterviewFeedback: analysisContext.submitInterviewFeedback,
    runInterviewEvaluation: analysisContext.runInterviewEvaluation,
    finalizeHiringDecision,
    loadAiSettings,
    saveAiSettings,
    testAiSettings,
    loadTaskSettings,
    saveTaskSettings,
    loadTaskPeople,
    syncTaskPeople,
    toggleTaskRunState,
    deleteTask,
    pauseTask,
    resumeTask,
    cancelTask,
  };
});
