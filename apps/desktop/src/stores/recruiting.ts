import { computed, ref } from "vue";
import { defineStore } from "pinia";
import type {
  AnalysisResult,
  CandidateRecord,
  CrawlMode,
  CrawlTaskRecord,
  DashboardMetrics,
  InterviewQuestion,
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
  listAnalysis,
  listCandidates,
  listCrawlTasks,
  listJobs,
  listPipelineEvents,
  loadDashboardMetrics,
  mergeCandidateImport,
  moveCandidateStage,
  parseResumeFile,
  getTaskRuntimeSettings,
  runCandidateAnalysis,
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
  type BackendAnalysisRecord,
  type ScreeningResultRecord,
  type ScreeningTemplateRecord,
  type UpsertScreeningTemplatePayload,
  type PipelineEvent,
  type InterviewEvaluationRecord,
  type InterviewFeedbackRecord,
  type InterviewKitRecord,
  type ParsedResumeFile,
  type SidecarQueueResult,
  type SearchHit,
  type TaskRuntimeSettings,
  generateInterviewKit as generateInterviewKitApi,
  getAiProviderSettings,
  getScreeningTemplate,
  listScreeningResults,
  runInterviewEvaluation as runInterviewEvaluationApi,
  runResumeScreening,
  saveInterviewKit as saveInterviewKitApi,
  submitInterviewFeedback as submitInterviewFeedbackApi,
  upsertResume,
  upsertScreeningTemplate,
  upsertAiProviderSettings,
} from "../services/backend";
import {
  type CandidateImportItem,
  extractCandidateImportItems,
  extractJobImportItems,
  extractResumeImportItem,
} from "../lib/crawl-import";

type UiAnalysisRecord = AnalysisResult & { id: number; createdAt: string };
type CandidateImportSource = "boss" | "zhilian" | "wuba" | "lagou";
type ConflictResolutionAction = "merge" | "create" | "skip";
type AutoProcessTarget = {
  localCandidateId: number;
  externalCandidateId: string;
};
type CandidateImportBatchResult = {
  fetchedRows: number;
  importedCandidates: CandidateRecord[];
  mergedCandidates: CandidateRecord[];
  conflicts: CandidateImportConflict[];
  skippedRows: number;
  autoProcessTargets: AutoProcessTarget[];
};

export interface CandidateImportConflict {
  id: string;
  source: CandidateImportSource;
  mode: CrawlMode;
  localJobId: number;
  existingCandidate: CandidateRecord;
  imported: CandidateImportItem;
  reasons: string[];
  createdAt: string;
}

export interface CandidateImportQualityReport {
  source: CandidateImportSource;
  localJobId: number;
  generatedAt: string;
  fetchedRows: number;
  importedRows: number;
  mergedRows: number;
  conflictRows: number;
  skippedRows: number;
  autoResumeProcessed: number;
  autoAnalysisTriggered: number;
  autoErrorCount: number;
}

type SidecarTaskError = Error & {
  sidecarErrorCode?: string;
  sidecarSnapshot?: Record<string, unknown>;
};

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

  function wait(delayMs: number): Promise<void> {
    if (delayMs <= 0) {
      return Promise.resolve();
    }

    return new Promise((resolve) => setTimeout(resolve, delayMs));
  }

  function setError(error: unknown) {
    if (error instanceof Error) {
      lastError.value = error.message;
      return;
    }
    lastError.value = "Unknown error";
  }

  function mapAnalysis(record: BackendAnalysisRecord): UiAnalysisRecord {
    const provider = typeof record.model_info.provider === "string"
      ? record.model_info.provider
      : "local-heuristic";
    const model = typeof record.model_info.model === "string"
      ? record.model_info.model
      : "local-scorecard-v1";
    const generatedAt = typeof record.model_info.generatedAt === "string"
      ? record.model_info.generatedAt
      : record.created_at;

    return {
      id: record.id,
      overallScore: record.overall_score,
      dimensionScores: record.dimension_scores.map((item) => ({
        key:
          item.key === "skill_match" ||
          item.key === "experience" ||
          item.key === "compensation" ||
          item.key === "stability"
            ? item.key
            : "skill_match",
        score: item.score,
        reason: item.reason,
      })),
      risks: record.risks,
      highlights: record.highlights,
      suggestions: record.suggestions,
      evidence: record.evidence.map((item) => ({
        dimension: item.dimension,
        statement: item.statement,
        sourceSnippet: item.source_snippet,
      })),
      modelInfo: {
        provider,
        model,
        generatedAt,
      },
      createdAt: record.created_at,
    };
  }

  function buildSidecarTaskError(result: SidecarQueueResult, fallbackCode: string): SidecarTaskError {
    const error = new Error(result.error ?? fallbackCode) as SidecarTaskError;
    error.sidecarErrorCode = result.errorCode ?? fallbackCode;
    if (result.snapshot && typeof result.snapshot === "object" && !Array.isArray(result.snapshot)) {
      error.sidecarSnapshot = result.snapshot;
    }

    return error;
  }

  function assertSidecarSucceeded(result: SidecarQueueResult, fallbackCode: string) {
    if (result.status === "FAILED") {
      throw buildSidecarTaskError(result, fallbackCode);
    }
  }

  function resolveTaskErrorCode(error: unknown, fallbackCode: string): string {
    if (error && typeof error === "object" && "sidecarErrorCode" in error) {
      const code = (error as { sidecarErrorCode?: unknown }).sidecarErrorCode;
      if (typeof code === "string" && code.trim()) {
        return code;
      }
    }

    if (error instanceof Error && error.message.trim()) {
      return error.message;
    }

    return fallbackCode;
  }

  function resolveTaskErrorSnapshot(error: unknown): Record<string, unknown> | undefined {
    if (error && typeof error === "object" && "sidecarSnapshot" in error) {
      const snapshot = (error as { sidecarSnapshot?: unknown }).sidecarSnapshot;
      if (snapshot && typeof snapshot === "object" && !Array.isArray(snapshot)) {
        return snapshot as Record<string, unknown>;
      }
    }

    return undefined;
  }

  function normalizeText(value: string | null | undefined): string {
    return (value ?? "")
      .trim()
      .toLowerCase()
      .replace(/\s+/g, " ");
  }

  function buildCandidateIdentityKey(item: {
    name: string;
    current_company?: string | null;
  }): string {
    return `${normalizeText(item.name)}|${normalizeText(item.current_company)}`;
  }

  function mergeConflictReasons(existing: CandidateRecord, incoming: CandidateImportItem): string[] {
    const reasons: string[] = [];
    const existingCompany = normalizeText(existing.current_company);
    const incomingCompany = normalizeText(incoming.current_company);

    if (existingCompany && incomingCompany && existingCompany !== incomingCompany) {
      reasons.push("company_mismatch");
    }

    if (Math.abs(existing.years_of_experience - incoming.years_of_experience) > 2) {
      reasons.push("years_gap_gt_2");
    }

    return reasons;
  }

  function replaceCandidateInStore(updated: CandidateRecord) {
    const index = candidates.value.findIndex((item) => item.id === updated.id);
    if (index >= 0) {
      candidates.value[index] = updated;
    } else {
      candidates.value.unshift(updated);
    }
  }

  async function fileToBase64(file: File): Promise<string> {
    const buffer = await file.arrayBuffer();
    const bytes = new Uint8Array(buffer);
    let binary = "";
    const chunkSize = 0x8000;
    for (let index = 0; index < bytes.length; index += chunkSize) {
      const chunk = bytes.subarray(index, index + chunkSize);
      binary += String.fromCharCode(...chunk);
    }

    return btoa(binary);
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

  async function saveResume(payload: {
    candidate_id: number;
    raw_text: string;
    parsed: Record<string, unknown>;
    job_id?: number;
  }) {
    await upsertResume({
      source: "manual",
      ...payload,
    });
    await runResumeScreening({
      candidate_id: payload.candidate_id,
      job_id: payload.job_id,
    });
    screeningResults.value[payload.candidate_id] = await listScreeningResults(payload.candidate_id);
  }

  async function analyzeCandidate(candidateId: number, jobId?: number) {
    await runCandidateAnalysis({
      candidate_id: candidateId,
      job_id: jobId,
    });
    analyses.value[candidateId] = (await listAnalysis(candidateId)).map(mapAnalysis);
  }

  async function loadCandidateContext(candidateId: number) {
    const [analysisData, eventData, screeningData] = await Promise.all([
      listAnalysis(candidateId),
      listPipelineEvents(candidateId),
      listScreeningResults(candidateId),
    ]);
    analyses.value[candidateId] = analysisData.map(mapAnalysis);
    pipelineEvents.value[candidateId] = eventData;
    screeningResults.value[candidateId] = screeningData;
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

  async function runSidecarJobCrawl(payload: {
    source: CandidateImportSource;
    mode: CrawlMode;
    keyword: string;
    city?: string;
  }) {
    let taskId: number | null = null;
    try {
      const task = await addCrawlTask({
        source: payload.source,
        mode: payload.mode,
        task_type: "crawl_jobs",
        payload: {
          keyword: payload.keyword,
          city: payload.city,
        },
      });
      taskId = task.id;

      await updateCrawlTask({
        task_id: task.id,
        status: "RUNNING",
      });

      const result = await triggerSidecarCrawlJobs(payload);
      assertSidecarSucceeded(result, "sidecar_job_crawl_failed");
      const imported = await importJobsFromSidecarResult(result, payload.source);

      await updateCrawlTask({
        task_id: task.id,
        status: "SUCCEEDED",
        snapshot: {
          sidecarStatus: result.status,
          importedJobs: imported.length,
          sidecarTaskId: result.id,
        },
      });

      await Promise.all([refreshTasks(), refreshMetrics()]);

      return {
        result,
        importedJobs: imported.length,
      };
    } catch (error) {
      if (taskId !== null) {
        const snapshot = resolveTaskErrorSnapshot(error);
        await updateCrawlTask({
          task_id: taskId,
          status: "FAILED",
          error_code: resolveTaskErrorCode(error, "sidecar_run_failed"),
          snapshot,
        });
        await Promise.all([refreshTasks(), refreshMetrics()]);
      }

      throw error;
    }
  }

  async function runSidecarCandidateCrawl(payload: {
    source: CandidateImportSource;
    mode: CrawlMode;
    localJobId: number;
    externalJobId?: string;
  }) {
    let taskId: number | null = null;
    try {
      const sidecarJobId = payload.externalJobId?.trim() || String(payload.localJobId);
      const task = await addCrawlTask({
        source: payload.source,
        mode: payload.mode,
        task_type: "crawl_candidates",
        payload: {
          sidecarJobId,
          localJobId: payload.localJobId,
        },
      });
      taskId = task.id;

      await updateCrawlTask({
        task_id: task.id,
        status: "RUNNING",
      });

      const result = await triggerSidecarCrawlCandidates({
        source: payload.source,
        mode: payload.mode,
        jobId: sidecarJobId,
      });
      assertSidecarSucceeded(result, "sidecar_candidate_crawl_failed");
      const imported = await importCandidatesFromSidecarResult(
        result,
        payload.source,
        payload.mode,
        payload.localJobId,
      );
      const autoSummary = await autoProcessImportedCandidates({
        targets: imported.autoProcessTargets,
        source: payload.source,
        mode: payload.mode,
        localJobId: payload.localJobId,
      });
      const qualityReport: CandidateImportQualityReport = {
        source: payload.source,
        localJobId: payload.localJobId,
        generatedAt: new Date().toISOString(),
        fetchedRows: imported.fetchedRows,
        importedRows: imported.importedCandidates.length,
        mergedRows: imported.mergedCandidates.length,
        conflictRows: imported.conflicts.length,
        skippedRows: imported.skippedRows,
        autoResumeProcessed: autoSummary.resumeAutoProcessed,
        autoAnalysisTriggered: autoSummary.analysisTriggered,
        autoErrorCount: autoSummary.errors.length,
      };
      lastCandidateImportReport.value = qualityReport;

      await updateCrawlTask({
        task_id: task.id,
        status: "SUCCEEDED",
        snapshot: {
          sidecarStatus: result.status,
          importedCandidates: imported.importedCandidates.length,
          mergedCandidates: imported.mergedCandidates.length,
          conflictCandidates: imported.conflicts.length,
          skippedCandidates: imported.skippedRows,
          resumeAutoProcessed: autoSummary.resumeAutoProcessed,
          analysisTriggered: autoSummary.analysisTriggered,
          autoProcessErrors: autoSummary.errors,
          importQualityReport: qualityReport,
          sidecarTaskId: result.id,
        },
      });

      await Promise.all([refreshTasks(), refreshMetrics()]);
      return {
        result,
        importedCandidates: imported.importedCandidates.length,
        mergedCandidates: imported.mergedCandidates.length,
        conflictCandidates: imported.conflicts.length,
        skippedCandidates: imported.skippedRows,
        resumeAutoProcessed: autoSummary.resumeAutoProcessed,
        analysisTriggered: autoSummary.analysisTriggered,
        autoProcessErrors: autoSummary.errors,
        qualityReport,
      };
    } catch (error) {
      if (taskId !== null) {
        const snapshot = resolveTaskErrorSnapshot(error);
        await updateCrawlTask({
          task_id: taskId,
          status: "FAILED",
          error_code: resolveTaskErrorCode(error, "sidecar_candidate_run_failed"),
          snapshot,
        });
        await Promise.all([refreshTasks(), refreshMetrics()]);
      }
      throw error;
    }
  }

  async function runSidecarResumeCrawl(payload: {
    source: CandidateImportSource;
    mode: CrawlMode;
    localCandidateId: number;
    externalCandidateId: string;
    localJobId?: number;
  }) {
    let taskId: number | null = null;
    try {
      const task = await addCrawlTask({
        source: payload.source,
        mode: payload.mode,
        task_type: "crawl_resume",
        payload: {
          localCandidateId: payload.localCandidateId,
          externalCandidateId: payload.externalCandidateId,
        },
      });
      taskId = task.id;

      await updateCrawlTask({
        task_id: task.id,
        status: "RUNNING",
      });

      const result = await triggerSidecarCrawlResume({
        source: payload.source,
        mode: payload.mode,
        candidateId: payload.externalCandidateId,
      });
      assertSidecarSucceeded(result, "sidecar_resume_crawl_failed");
      const resume = extractResumeImportItem(result);
      if (!resume) {
        throw new Error("No resume payload available from sidecar result");
      }

      await saveResume({
        candidate_id: payload.localCandidateId,
        raw_text: resume.raw_text,
        parsed: resume.parsed,
        job_id: payload.localJobId,
      });

      await updateCrawlTask({
        task_id: task.id,
        status: "SUCCEEDED",
        snapshot: {
          sidecarStatus: result.status,
          rawTextLength: resume.raw_text.length,
          sidecarTaskId: result.id,
        },
      });

      await Promise.all([
        refreshTasks(),
        loadCandidateContext(payload.localCandidateId),
      ]);

      return {
        result,
        resumeImported: true,
      };
    } catch (error) {
      if (taskId !== null) {
        const snapshot = resolveTaskErrorSnapshot(error);
        await updateCrawlTask({
          task_id: taskId,
          status: "FAILED",
          error_code: resolveTaskErrorCode(error, "sidecar_resume_run_failed"),
          snapshot,
        });
        await Promise.all([refreshTasks(), refreshMetrics()]);
      }
      throw error;
    }
  }

  async function importJobsFromSidecarResult(
    result: SidecarQueueResult,
    source: CandidateImportSource,
  ): Promise<JobRecord[]> {
    const importItems = extractJobImportItems(result);
    if (importItems.length === 0) {
      return [];
    }

    const existingByExternalId = new Set(
      jobs.value
        .map((item) => item.external_id)
        .filter((item): item is string => Boolean(item)),
    );
    const existingByIdentity = new Set(
      jobs.value.map((item) => `${item.source}:${item.title}:${item.company}:${item.city ?? ""}`),
    );

    const inserted: JobRecord[] = [];
    for (const item of importItems) {
      const identity = `${source}:${item.title}:${item.company}:${item.city ?? ""}`;
      if (item.external_id && existingByExternalId.has(item.external_id)) {
        continue;
      }
      if (existingByIdentity.has(identity)) {
        continue;
      }

      const created = await createJob({
        source,
        external_id: item.external_id,
        title: item.title,
        company: item.company,
        city: item.city,
        salary_k: item.salary_k,
        description: item.description,
      });
      inserted.push(created);
      jobs.value.unshift(created);
      if (created.external_id) {
        existingByExternalId.add(created.external_id);
      }
      existingByIdentity.add(identity);
    }

    return inserted;
  }

  async function importCandidatesFromSidecarResult(
    result: SidecarQueueResult,
    source: CandidateImportSource,
    mode: CrawlMode,
    localJobId: number,
  ): Promise<CandidateImportBatchResult> {
    const importItems = extractCandidateImportItems(result);
    const mergeTag = `source:${source}`;
    const fetchedRows = importItems.length;
    if (importItems.length === 0) {
      return {
        fetchedRows: 0,
        importedCandidates: [],
        mergedCandidates: [],
        conflicts: [],
        skippedRows: 0,
        autoProcessTargets: [],
      };
    }

    const existingByExternalId = new Set(
      candidates.value
        .map((item) => item.external_id)
        .filter((item): item is string => Boolean(item)),
    );
    const existingByIdentity = new Map<string, CandidateRecord[]>();
    for (const candidate of candidates.value) {
      const key = buildCandidateIdentityKey(candidate);
      const list = existingByIdentity.get(key) ?? [];
      list.push(candidate);
      existingByIdentity.set(key, list);
    }

    const inserted: CandidateRecord[] = [];
    const merged: CandidateRecord[] = [];
    const conflicts: CandidateImportConflict[] = [];
    const autoProcessTargets: AutoProcessTarget[] = [];
    let skippedRows = 0;

    for (const item of importItems) {
      if (item.external_id && existingByExternalId.has(item.external_id)) {
        skippedRows += 1;
        continue;
      }

      const identity = buildCandidateIdentityKey(item);
      const identityMatches = existingByIdentity.get(identity) ?? [];

      if (identityMatches.length === 1) {
        const target = identityMatches[0];
        const reasons = mergeConflictReasons(target, item);
        if (reasons.length > 0) {
          conflicts.push({
            id: `${target.id}-${Date.now()}-${conflicts.length}`,
            source,
            mode,
            localJobId,
            existingCandidate: target,
            imported: item,
            reasons,
            createdAt: new Date().toISOString(),
          });
          continue;
        }

        const mergedRecord = await mergeCandidateImport({
          candidate_id: target.id,
          current_company: item.current_company,
          years_of_experience: item.years_of_experience,
          tags: mergeTag ? [...item.tags, mergeTag] : item.tags,
          phone: item.phone,
          email: item.email,
          job_id: localJobId,
        });
        replaceCandidateInStore(mergedRecord);
        merged.push(mergedRecord);
        if (item.external_id) {
          autoProcessTargets.push({
            localCandidateId: mergedRecord.id,
            externalCandidateId: item.external_id,
          });
        }
        continue;
      }

      if (identityMatches.length > 1) {
        conflicts.push({
          id: `multi-${identity}-${Date.now()}-${conflicts.length}`,
          source,
          mode,
          localJobId,
          existingCandidate: identityMatches[0],
          imported: item,
          reasons: ["multiple_identity_matches"],
          createdAt: new Date().toISOString(),
        });
        continue;
      }

      const created = await createCandidate({
        source,
        external_id: item.external_id,
        name: item.name,
        current_company: item.current_company,
        years_of_experience: item.years_of_experience,
        tags: item.tags,
        phone: item.phone,
        email: item.email,
        job_id: localJobId,
      });
      inserted.push(created);
      candidates.value.unshift(created);
      if (created.external_id) {
        existingByExternalId.add(created.external_id);
      }
      if (created.external_id) {
        autoProcessTargets.push({
          localCandidateId: created.id,
          externalCandidateId: created.external_id,
        });
      }
      const list = existingByIdentity.get(identity) ?? [];
      list.push(created);
      existingByIdentity.set(identity, list);
    }

    if (conflicts.length > 0) {
      candidateImportConflicts.value = [...conflicts, ...candidateImportConflicts.value];
    }

    return {
      fetchedRows,
      importedCandidates: inserted,
      mergedCandidates: merged,
      conflicts,
      skippedRows,
      autoProcessTargets,
    };
  }

  async function autoProcessImportedCandidates(payload: {
    targets: AutoProcessTarget[];
    source: CandidateImportSource;
    mode: CrawlMode;
    localJobId: number;
  }): Promise<{
    resumeAutoProcessed: number;
    analysisTriggered: number;
    errors: Array<{ candidateId: number; message: string }>;
  }> {
    let resumeAutoProcessed = 0;
    let analysisTriggered = 0;
    const errors: Array<{ candidateId: number; message: string }> = [];
    const targets = payload.targets.filter((target) => Boolean(target.externalCandidateId));
    const concurrency = Math.max(1, taskSettings.value.auto_batch_concurrency);
    const retryCount = Math.max(0, taskSettings.value.auto_retry_count);
    const retryBackoff = Math.max(100, taskSettings.value.auto_retry_backoff_ms);

    let currentIndex = 0;
    async function withRetry<T>(runner: () => Promise<T>): Promise<T> {
      let attempt = 0;
      let lastError: unknown = null;
      while (attempt <= retryCount) {
        try {
          return await runner();
        } catch (error) {
          lastError = error;
          if (attempt >= retryCount) {
            break;
          }
          await wait(retryBackoff * (attempt + 1));
        }
        attempt += 1;
      }
      throw lastError instanceof Error ? lastError : new Error("task_retry_failed");
    }

    async function worker() {
      while (currentIndex < targets.length) {
        const target = targets[currentIndex];
        currentIndex += 1;
        if (!target?.externalCandidateId) {
          continue;
        }

        try {
          const resumeResponse = await withRetry(() => runSidecarResumeCrawl({
            source: payload.source,
            mode: payload.mode,
            localCandidateId: target.localCandidateId,
            externalCandidateId: target.externalCandidateId,
            localJobId: payload.localJobId,
          }));

          if (resumeResponse.resumeImported) {
            resumeAutoProcessed += 1;
          }

          await withRetry(() => analyzeCandidate(target.localCandidateId, payload.localJobId));
          analysisTriggered += 1;
        } catch (error) {
          errors.push({
            candidateId: target.localCandidateId,
            message: error instanceof Error ? error.message : "auto_resume_or_analysis_failed",
          });
        }
      }
    }

    const workers = Array.from({ length: Math.min(concurrency, Math.max(targets.length, 1)) }, () => worker());
    await Promise.all(workers);

    return {
      resumeAutoProcessed,
      analysisTriggered,
      errors,
    };
  }

  async function resolveCandidateImportConflict(payload: {
    conflictId: string;
    action: ConflictResolutionAction;
  }) {
    const index = candidateImportConflicts.value.findIndex((item) => item.id === payload.conflictId);
    if (index < 0) {
      throw new Error("candidate_import_conflict_not_found");
    }

    const conflict = candidateImportConflicts.value[index];
    const autoTargets: AutoProcessTarget[] = [];

    if (payload.action === "merge") {
      const merged = await mergeCandidateImport({
        candidate_id: conflict.existingCandidate.id,
        current_company: conflict.imported.current_company,
        years_of_experience: conflict.imported.years_of_experience,
        tags: [...conflict.imported.tags, `source:${conflict.source}`],
        phone: conflict.imported.phone,
        email: conflict.imported.email,
        job_id: conflict.localJobId,
      });
      replaceCandidateInStore(merged);
      if (conflict.imported.external_id) {
        autoTargets.push({
          localCandidateId: merged.id,
          externalCandidateId: conflict.imported.external_id,
        });
      }
    }

    if (payload.action === "create") {
      const created = await createCandidate({
        source: conflict.source,
        external_id: conflict.imported.external_id,
        name: conflict.imported.name,
        current_company: conflict.imported.current_company,
        years_of_experience: conflict.imported.years_of_experience,
        tags: [...conflict.imported.tags, `source:${conflict.source}`],
        phone: conflict.imported.phone,
        email: conflict.imported.email,
        job_id: conflict.localJobId,
      });
      candidates.value.unshift(created);
      if (created.external_id) {
        autoTargets.push({
          localCandidateId: created.id,
          externalCandidateId: created.external_id,
        });
      }
    }

    if (autoTargets.length > 0) {
      await autoProcessImportedCandidates({
        targets: autoTargets,
        source: conflict.source,
        mode: conflict.mode,
        localJobId: conflict.localJobId,
      });
    }

    candidateImportConflicts.value.splice(index, 1);
    await Promise.all([refreshMetrics(), refreshTasks()]);

    return {
      action: payload.action,
      conflictId: payload.conflictId,
      remainingConflicts: candidateImportConflicts.value.length,
    };
  }

  async function search(query: string) {
    if (!query.trim()) {
      searchResults.value = [];
      return;
    }
    searchResults.value = await searchCandidates(query);
  }

  async function importResumeFileAndAnalyze(payload: {
    candidateId: number;
    file: File;
    enableOcr?: boolean;
    jobId?: number;
  }): Promise<ParsedResumeFile> {
    const contentBase64 = await fileToBase64(payload.file);
    const parsedFile = await parseResumeFile({
      file_name: payload.file.name,
      content_base64: contentBase64,
      enable_ocr: payload.enableOcr,
    });

    await saveResume({
      candidate_id: payload.candidateId,
      raw_text: parsedFile.raw_text,
      parsed: parsedFile.parsed,
      job_id: payload.jobId,
    });
    await analyzeCandidate(payload.candidateId, payload.jobId);
    await loadCandidateContext(payload.candidateId);

    return parsedFile;
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

  async function runScreening(candidateId: number, jobId?: number) {
    await runResumeScreening({
      candidate_id: candidateId,
      job_id: jobId,
    });
    screeningResults.value[candidateId] = await listScreeningResults(candidateId);
    return screeningResults.value[candidateId];
  }

  async function generateInterviewKit(candidateId: number, jobId?: number) {
    const kit = await generateInterviewKitApi({
      candidate_id: candidateId,
      job_id: jobId,
    });
    interviewKits.value[candidateId] = kit;
    return kit;
  }

  async function saveInterviewKit(payload: {
    candidate_id: number;
    job_id?: number;
    questions: InterviewQuestion[];
  }) {
    const kit = await saveInterviewKitApi(payload);
    interviewKits.value[payload.candidate_id] = kit;
    return kit;
  }

  async function submitInterviewFeedback(payload: {
    candidate_id: number;
    job_id?: number;
    transcript_text: string;
    structured_feedback: Record<string, unknown>;
    recording_path?: string;
  }) {
    const feedback = await submitInterviewFeedbackApi(payload);
    const existing = interviewFeedback.value[payload.candidate_id] ?? [];
    interviewFeedback.value[payload.candidate_id] = [feedback, ...existing];
    return feedback;
  }

  async function runInterviewEvaluation(payload: {
    candidate_id: number;
    job_id?: number;
    feedback_id?: number;
  }) {
    const evaluation = await runInterviewEvaluationApi(payload);
    const existing = interviewEvaluations.value[payload.candidate_id] ?? [];
    interviewEvaluations.value[payload.candidate_id] = [evaluation, ...existing];
    return evaluation;
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
    saveResume,
    analyzeCandidate,
    loadCandidateContext,
    addCrawlTask,
    runSidecarJobCrawl,
    runSidecarCandidateCrawl,
    runSidecarResumeCrawl,
    resolveCandidateImportConflict,
    search,
    importResumeFileAndAnalyze,
    loadScreeningTemplate,
    saveScreeningTemplate,
    runScreening,
    generateInterviewKit,
    saveInterviewKit,
    submitInterviewFeedback,
    runInterviewEvaluation,
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
