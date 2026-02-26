import { computed, ref } from "vue";
import { defineStore } from "pinia";
import type {
  CandidateRecord,
  CrawlMode,
  CrawlTaskRecord,
  DashboardMetrics,
  JobRecord,
  PipelineStage,
} from "@doss/shared";
import {
  checkSidecarHealth,
  ensureSidecar,
  createCandidate,
  createCrawlTask,
  createJob,
  getHealth,
  listCandidates,
  listCrawlTasks,
  listJobs,
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
  upsertTaskRuntimeSettings,
  updateCrawlTask,
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
  type InterviewKitRecord,
  type SearchHit,
  type TaskRuntimeSettings,
  generateInterviewKit as generateInterviewKitApi,
  getAiProviderSettings,
  getScreeningTemplate,
  listAnalysis,
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
import {
  createAnalysisContextModule,
  mapBackendAnalysisRecord,
} from "./recruiting/analysis-context";
import { createCandidateImportModule } from "./recruiting/candidate-import";
import { createTaskOrchestrator } from "./recruiting/task-orchestrator";
import type {
  CandidateImportConflict,
  CandidateImportQualityReport,
  CandidateImportSource,
  ConflictResolutionAction,
  UiAnalysisRecord,
} from "./recruiting/types";

export type {
  CandidateImportConflict,
  CandidateImportQualityReport,
} from "./recruiting/types";

export const useRecruitingStore = defineStore("recruiting", () => {
  const loading = ref(false);
  const lastError = ref<string | null>(null);

  const jobs = ref<JobRecord[]>([]);
  const candidates = ref<CandidateRecord[]>([]);
  const tasks = ref<CrawlTaskRecord[]>([]);
  const metrics = ref<DashboardMetrics | null>(null);
  const health = ref<AppHealth | null>(null);
  const sidecarHealthy = ref<boolean | null>(null);

  const analyses = ref<Record<number, UiAnalysisRecord[]>>({});
  const screeningResults = ref<Record<number, ScreeningResultRecord[]>>({});
  const interviewKits = ref<Record<number, InterviewKitRecord | null>>({});
  const interviewFeedback = ref<Record<number, InterviewFeedbackRecord[]>>({});
  const interviewEvaluations = ref<Record<number, InterviewEvaluationRecord[]>>({});
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

  const hasBootstrapped = ref(false);
  const stageSummary = computed(() => metrics.value?.stage_stats ?? []);

  function setError(error: unknown) {
    if (error instanceof Error) {
      lastError.value = error.message;
      return;
    }
    lastError.value = "Unknown error";
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
      metrics.value = metricsData;
      health.value = healthData;

      try {
        await ensureSidecar();
        const sidecar = await checkSidecarHealth();
        sidecarHealthy.value = sidecar.ok;
      } catch {
        sidecarHealthy.value = false;
      }

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

  async function refreshTasks() {
    tasks.value = await listCrawlTasks();
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
    jobs.value.unshift(job);
    await refreshMetrics();
    return job;
  }

  async function addCandidate(payload: {
    name: string;
    current_company?: string;
    years_of_experience: number;
    phone?: string;
    email?: string;
    tags: string[];
    job_id?: number;
  }) {
    const candidate = await createCandidate({
      source: "manual",
      ...payload,
    });
    candidates.value.unshift(candidate);
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

  const analysisContext = createAnalysisContextModule({
    analyses,
    screeningResults,
    interviewKits,
    interviewFeedback,
    interviewEvaluations,
    pipelineEvents,
    mapAnalysis: mapBackendAnalysisRecord,
    runCandidateAnalysis,
    listAnalysis,
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

  async function pauseTask(taskId: number) {
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
    metrics,
    health,
    sidecarHealthy,
    analyses,
    screeningResults,
    interviewKits,
    interviewFeedback,
    interviewEvaluations,
    pipelineEvents,
    searchResults,
    aiSettings,
    activeScreeningTemplate,
    taskSettings,
    candidateImportConflicts,
    lastCandidateImportReport,
    hasBootstrapped,
    stageSummary,
    bootstrap,
    refreshMetrics,
    refreshTasks,
    addJob,
    addCandidate,
    moveStage,
    saveResume: analysisContext.saveResume,
    analyzeCandidate: analysisContext.analyzeCandidate,
    loadCandidateContext: analysisContext.loadCandidateContext,
    addCrawlTask,
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
    runScreening: analysisContext.runScreening,
    generateInterviewKit: analysisContext.generateInterviewKit,
    saveInterviewKit: analysisContext.saveInterviewKit,
    submitInterviewFeedback: analysisContext.submitInterviewFeedback,
    runInterviewEvaluation: analysisContext.runInterviewEvaluation,
    loadAiSettings,
    saveAiSettings,
    testAiSettings,
    loadTaskSettings,
    saveTaskSettings,
    pauseTask,
    resumeTask,
    cancelTask,
  };
});
