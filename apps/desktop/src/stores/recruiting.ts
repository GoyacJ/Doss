import { computed, ref } from "vue";
import { defineStore } from "pinia";
import type {
  AnalysisResult,
  CandidateRecord,
  CrawlMode,
  CrawlTaskRecord,
  DashboardMetrics,
  JobRecord,
  PipelineStage,
} from "@doss/shared";
import {
  checkSidecarHealth,
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
  moveCandidateStage,
  runCandidateAnalysis,
  searchCandidates,
  triggerSidecarCrawlCandidates,
  triggerSidecarCrawlJobs,
  triggerSidecarCrawlResume,
  updateCrawlTask,
  type AppHealth,
  type BackendAnalysisRecord,
  type PipelineEvent,
  type SidecarQueueResult,
  type SearchHit,
  upsertResume,
} from "../services/backend";
import {
  extractCandidateImportItems,
  extractJobImportItems,
  extractResumeImportItem,
} from "../lib/crawl-import";

type UiAnalysisRecord = AnalysisResult & { id: number; createdAt: string };

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
  const pipelineEvents = ref<Record<number, PipelineEvent[]>>({});
  const searchResults = ref<SearchHit[]>([]);

  const hasBootstrapped = ref(false);

  const stageSummary = computed(() => metrics.value?.stage_stats ?? []);

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
      : "cloud-mock";
    const model = typeof record.model_info.model === "string"
      ? record.model_info.model
      : "gpt-style-compat";
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
        const sidecar = await checkSidecarHealth();
        sidecarHealthy.value = sidecar.ok;
      } catch {
        sidecarHealthy.value = false;
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
  }) {
    await upsertResume({
      source: "manual",
      ...payload,
    });
  }

  async function analyzeCandidate(candidateId: number, jobId?: number) {
    await runCandidateAnalysis({
      candidate_id: candidateId,
      job_id: jobId,
    });
    analyses.value[candidateId] = (await listAnalysis(candidateId)).map(mapAnalysis);
  }

  async function loadCandidateContext(candidateId: number) {
    const [analysisData, eventData] = await Promise.all([
      listAnalysis(candidateId),
      listPipelineEvents(candidateId),
    ]);
    analyses.value[candidateId] = analysisData.map(mapAnalysis);
    pipelineEvents.value[candidateId] = eventData;
  }

  async function addCrawlTask(payload: {
    source: "boss" | "zhilian" | "wuba";
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
    source: "boss" | "zhilian" | "wuba";
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
        await updateCrawlTask({
          task_id: taskId,
          status: "FAILED",
          error_code: error instanceof Error ? error.message : "sidecar_run_failed",
        });
        await Promise.all([refreshTasks(), refreshMetrics()]);
      }

      throw error;
    }
  }

  async function runSidecarCandidateCrawl(payload: {
    source: "boss" | "zhilian" | "wuba";
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
      const imported = await importCandidatesFromSidecarResult(
        result,
        payload.source,
        payload.localJobId,
      );
      const autoSummary = await autoProcessImportedCandidates({
        candidates: imported,
        source: payload.source,
        mode: payload.mode,
        localJobId: payload.localJobId,
      });

      await updateCrawlTask({
        task_id: task.id,
        status: "SUCCEEDED",
        snapshot: {
          sidecarStatus: result.status,
          importedCandidates: imported.length,
          resumeAutoProcessed: autoSummary.resumeAutoProcessed,
          analysisTriggered: autoSummary.analysisTriggered,
          autoProcessErrors: autoSummary.errors,
          sidecarTaskId: result.id,
        },
      });

      await Promise.all([refreshTasks(), refreshMetrics()]);
      return {
        result,
        importedCandidates: imported.length,
        resumeAutoProcessed: autoSummary.resumeAutoProcessed,
        analysisTriggered: autoSummary.analysisTriggered,
        autoProcessErrors: autoSummary.errors,
      };
    } catch (error) {
      if (taskId !== null) {
        await updateCrawlTask({
          task_id: taskId,
          status: "FAILED",
          error_code: error instanceof Error ? error.message : "sidecar_candidate_run_failed",
        });
        await Promise.all([refreshTasks(), refreshMetrics()]);
      }
      throw error;
    }
  }

  async function runSidecarResumeCrawl(payload: {
    source: "boss" | "zhilian" | "wuba";
    mode: CrawlMode;
    localCandidateId: number;
    externalCandidateId: string;
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
      const resume = extractResumeImportItem(result);
      if (!resume) {
        throw new Error("No resume payload available from sidecar result");
      }

      await saveResume({
        candidate_id: payload.localCandidateId,
        raw_text: resume.raw_text,
        parsed: resume.parsed,
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
        await updateCrawlTask({
          task_id: taskId,
          status: "FAILED",
          error_code: error instanceof Error ? error.message : "sidecar_resume_run_failed",
        });
        await Promise.all([refreshTasks(), refreshMetrics()]);
      }
      throw error;
    }
  }

  async function importJobsFromSidecarResult(
    result: SidecarQueueResult,
    source: "boss" | "zhilian" | "wuba",
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
    source: "boss" | "zhilian" | "wuba",
    localJobId: number,
  ): Promise<CandidateRecord[]> {
    const importItems = extractCandidateImportItems(result);
    if (importItems.length === 0) {
      return [];
    }

    const existingByExternalId = new Set(
      candidates.value
        .map((item) => item.external_id)
        .filter((item): item is string => Boolean(item)),
    );
    const existingByIdentity = new Set(
      candidates.value.map(
        (item) =>
          `${item.source}:${item.name}:${item.current_company ?? ""}:${item.years_of_experience}`,
      ),
    );

    const inserted: CandidateRecord[] = [];
    for (const item of importItems) {
      const identity = `${source}:${item.name}:${item.current_company ?? ""}:${item.years_of_experience}`;
      if (item.external_id && existingByExternalId.has(item.external_id)) {
        continue;
      }
      if (existingByIdentity.has(identity)) {
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
      existingByIdentity.add(identity);
    }

    return inserted;
  }

  async function autoProcessImportedCandidates(payload: {
    candidates: CandidateRecord[];
    source: "boss" | "zhilian" | "wuba";
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

    for (const candidate of payload.candidates) {
      if (!candidate.external_id) {
        continue;
      }

      try {
        const resumeResponse = await runSidecarResumeCrawl({
          source: payload.source,
          mode: payload.mode,
          localCandidateId: candidate.id,
          externalCandidateId: candidate.external_id,
        });

        if (resumeResponse.resumeImported) {
          resumeAutoProcessed += 1;
        }

        await analyzeCandidate(candidate.id, payload.localJobId);
        analysisTriggered += 1;
      } catch (error) {
        errors.push({
          candidateId: candidate.id,
          message: error instanceof Error ? error.message : "auto_resume_or_analysis_failed",
        });
      }
    }

    return {
      resumeAutoProcessed,
      analysisTriggered,
      errors,
    };
  }

  async function search(query: string) {
    if (!query.trim()) {
      searchResults.value = [];
      return;
    }
    searchResults.value = await searchCandidates(query);
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
    pipelineEvents,
    searchResults,
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
    search,
  };
});
