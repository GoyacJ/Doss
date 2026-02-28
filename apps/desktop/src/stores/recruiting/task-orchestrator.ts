import type { Ref } from "vue";
import type { CandidateRecord, CrawlMode, JobRecord } from "@doss/shared";
import { extractResumeImportItem } from "../../lib/crawl-import";
import type {
  SidecarQueueResult,
  TaskRuntimeSettings,
} from "../../services/backend";
import type {
  AutoProcessTarget,
  CandidateImportConflict,
  CandidateImportQualityReport,
  CandidateImportSource,
  SidecarTaskError,
} from "./types";

function wait(delayMs: number): Promise<void> {
  if (delayMs <= 0) {
    return Promise.resolve();
  }

  return new Promise((resolve) => setTimeout(resolve, delayMs));
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

function replaceCandidateInStore(candidates: Ref<CandidateRecord[]>, updated: CandidateRecord) {
  const index = candidates.value.findIndex((item) => item.id === updated.id);
  if (index >= 0) {
    candidates.value[index] = updated;
  } else {
    candidates.value.unshift(updated);
  }
}

export interface TaskOrchestratorDeps {
  candidates: Ref<CandidateRecord[]>;
  taskSettings: Ref<TaskRuntimeSettings>;
  candidateImportConflicts: Ref<CandidateImportConflict[]>;
  lastCandidateImportReport: Ref<CandidateImportQualityReport | null>;
  addCrawlTask: (payload: {
    source: CandidateImportSource;
    mode: CrawlMode;
    task_type: string;
    payload: Record<string, unknown>;
  }) => Promise<{ id: number }>;
  updateCrawlTask: (payload: {
    task_id: number;
    status: "PENDING" | "RUNNING" | "PAUSED" | "CANCELED" | "SUCCEEDED" | "FAILED";
    retry_count?: number;
    error_code?: string;
    snapshot?: Record<string, unknown>;
  }) => Promise<unknown>;
  refreshTasks: () => Promise<void>;
  refreshMetrics: () => Promise<void>;
  importJobsFromSidecarResult: (
    result: SidecarQueueResult,
    source: CandidateImportSource,
  ) => Promise<JobRecord[]>;
  importCandidatesFromSidecarResult: (
    result: SidecarQueueResult,
    source: CandidateImportSource,
    mode: CrawlMode,
    localJobId: number,
  ) => Promise<{
    fetchedRows: number;
    importedCandidates: CandidateRecord[];
    mergedCandidates: CandidateRecord[];
    conflicts: CandidateImportConflict[];
    skippedRows: number;
    autoProcessTargets: AutoProcessTarget[];
  }>;
  triggerSidecarCrawlJobs: (payload: {
    source: CandidateImportSource;
    mode: CrawlMode;
    keyword: string;
    city?: string;
  }) => Promise<SidecarQueueResult>;
  triggerSidecarCrawlCandidates: (payload: {
    source: CandidateImportSource;
    mode: CrawlMode;
    jobId: string;
  }) => Promise<SidecarQueueResult>;
  triggerSidecarCrawlResume: (payload: {
    source: CandidateImportSource;
    mode: CrawlMode;
    candidateId: string;
  }) => Promise<SidecarQueueResult>;
  saveResume: (payload: {
    candidate_id: number;
    raw_text: string;
    parsed: Record<string, unknown>;
    job_id?: number;
    source?: CandidateImportSource;
  }) => Promise<void>;
  loadCandidateContext: (candidateId: number) => Promise<void>;
  analyzeCandidate: (candidateId: number, jobId?: number) => Promise<void>;
  mergeCandidateImport: (payload: {
    candidate_id: number;
    current_company?: string;
    years_of_experience?: number;
    address?: string;
    tags?: string[];
    phone?: string;
    email?: string;
    job_id?: number;
  }) => Promise<CandidateRecord>;
  createCandidate: (payload: {
    source: CandidateImportSource;
    external_id?: string;
    name: string;
    current_company?: string;
    years_of_experience: number;
    age?: number;
    address?: string;
    phone?: string;
    email?: string;
    tags: string[];
    job_id?: number;
  }) => Promise<CandidateRecord>;
}

export function createTaskOrchestrator(deps: TaskOrchestratorDeps) {
  async function runSidecarJobCrawl(payload: {
    source: CandidateImportSource;
    mode: CrawlMode;
    keyword: string;
    city?: string;
  }) {
    let taskId: number | null = null;
    try {
      const task = await deps.addCrawlTask({
        source: payload.source,
        mode: payload.mode,
        task_type: "crawl_jobs",
        payload: {
          keyword: payload.keyword,
          city: payload.city,
        },
      });
      taskId = task.id;

      await deps.updateCrawlTask({
        task_id: task.id,
        status: "RUNNING",
      });

      const result = await deps.triggerSidecarCrawlJobs(payload);
      assertSidecarSucceeded(result, "sidecar_job_crawl_failed");
      const imported = await deps.importJobsFromSidecarResult(result, payload.source);

      await deps.updateCrawlTask({
        task_id: task.id,
        status: "SUCCEEDED",
        snapshot: {
          sidecarStatus: result.status,
          importedJobs: imported.length,
          sidecarTaskId: result.id,
        },
      });

      await Promise.all([deps.refreshTasks(), deps.refreshMetrics()]);

      return {
        result,
        importedJobs: imported.length,
      };
    } catch (error) {
      if (taskId !== null) {
        const snapshot = resolveTaskErrorSnapshot(error);
        await deps.updateCrawlTask({
          task_id: taskId,
          status: "FAILED",
          error_code: resolveTaskErrorCode(error, "sidecar_run_failed"),
          snapshot,
        });
        await Promise.all([deps.refreshTasks(), deps.refreshMetrics()]);
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
      const task = await deps.addCrawlTask({
        source: payload.source,
        mode: payload.mode,
        task_type: "crawl_resume",
        payload: {
          localCandidateId: payload.localCandidateId,
          externalCandidateId: payload.externalCandidateId,
        },
      });
      taskId = task.id;

      await deps.updateCrawlTask({
        task_id: task.id,
        status: "RUNNING",
      });

      const result = await deps.triggerSidecarCrawlResume({
        source: payload.source,
        mode: payload.mode,
        candidateId: payload.externalCandidateId,
      });
      assertSidecarSucceeded(result, "sidecar_resume_crawl_failed");
      const resume = extractResumeImportItem(result);
      if (!resume) {
        throw new Error("No resume payload available from sidecar result");
      }

      await deps.saveResume({
        candidate_id: payload.localCandidateId,
        raw_text: resume.raw_text,
        parsed: resume.parsed,
        job_id: payload.localJobId,
        source: payload.source,
      });

      await deps.updateCrawlTask({
        task_id: task.id,
        status: "SUCCEEDED",
        snapshot: {
          sidecarStatus: result.status,
          rawTextLength: resume.raw_text.length,
          sidecarTaskId: result.id,
        },
      });

      await Promise.all([
        deps.refreshTasks(),
        deps.loadCandidateContext(payload.localCandidateId),
      ]);

      return {
        result,
        resumeImported: true,
      };
    } catch (error) {
      if (taskId !== null) {
        const snapshot = resolveTaskErrorSnapshot(error);
        await deps.updateCrawlTask({
          task_id: taskId,
          status: "FAILED",
          error_code: resolveTaskErrorCode(error, "sidecar_resume_run_failed"),
          snapshot,
        });
        await Promise.all([deps.refreshTasks(), deps.refreshMetrics()]);
      }
      throw error;
    }
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
    const concurrency = Math.max(1, deps.taskSettings.value.auto_batch_concurrency);
    const retryCount = Math.max(0, deps.taskSettings.value.auto_retry_count);
    const retryBackoff = Math.max(100, deps.taskSettings.value.auto_retry_backoff_ms);

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

  async function runSidecarCandidateCrawl(payload: {
    source: CandidateImportSource;
    mode: CrawlMode;
    localJobId: number;
    externalJobId?: string;
  }) {
    let taskId: number | null = null;
    try {
      const sidecarJobId = payload.externalJobId?.trim() || String(payload.localJobId);
      const task = await deps.addCrawlTask({
        source: payload.source,
        mode: payload.mode,
        task_type: "crawl_candidates",
        payload: {
          sidecarJobId,
          localJobId: payload.localJobId,
        },
      });
      taskId = task.id;

      await deps.updateCrawlTask({
        task_id: task.id,
        status: "RUNNING",
      });

      const result = await deps.triggerSidecarCrawlCandidates({
        source: payload.source,
        mode: payload.mode,
        jobId: sidecarJobId,
      });
      assertSidecarSucceeded(result, "sidecar_candidate_crawl_failed");
      const imported = await deps.importCandidatesFromSidecarResult(
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
      deps.lastCandidateImportReport.value = qualityReport;

      await deps.updateCrawlTask({
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

      await Promise.all([deps.refreshTasks(), deps.refreshMetrics()]);
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
        await deps.updateCrawlTask({
          task_id: taskId,
          status: "FAILED",
          error_code: resolveTaskErrorCode(error, "sidecar_candidate_run_failed"),
          snapshot,
        });
        await Promise.all([deps.refreshTasks(), deps.refreshMetrics()]);
      }
      throw error;
    }
  }

  async function resolveCandidateImportConflict(payload: {
    conflictId: string;
    action: "merge" | "create" | "skip";
  }) {
    const index = deps.candidateImportConflicts.value.findIndex((item) => item.id === payload.conflictId);
    if (index < 0) {
      throw new Error("candidate_import_conflict_not_found");
    }

    const conflict = deps.candidateImportConflicts.value[index];
    const autoTargets: AutoProcessTarget[] = [];

    if (payload.action === "merge") {
      const merged = await deps.mergeCandidateImport({
        candidate_id: conflict.existingCandidate.id,
        current_company: conflict.imported.current_company,
        years_of_experience: conflict.imported.years_of_experience,
        address: conflict.imported.address,
        tags: [...conflict.imported.tags, `source:${conflict.source}`],
        phone: conflict.imported.phone,
        email: conflict.imported.email,
        job_id: conflict.localJobId,
      });
      replaceCandidateInStore(deps.candidates, merged);
      if (conflict.imported.external_id) {
        autoTargets.push({
          localCandidateId: merged.id,
          externalCandidateId: conflict.imported.external_id,
        });
      }
    }

    if (payload.action === "create") {
      const created = await deps.createCandidate({
        source: conflict.source,
        external_id: conflict.imported.external_id,
        name: conflict.imported.name,
        current_company: conflict.imported.current_company,
        years_of_experience: conflict.imported.years_of_experience,
        age: conflict.imported.age,
        address: conflict.imported.address,
        tags: [...conflict.imported.tags, `source:${conflict.source}`],
        phone: conflict.imported.phone,
        email: conflict.imported.email,
        job_id: conflict.localJobId,
      });
      deps.candidates.value.unshift(created);
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

    deps.candidateImportConflicts.value.splice(index, 1);
    await Promise.all([deps.refreshMetrics(), deps.refreshTasks()]);

    return {
      action: payload.action,
      conflictId: payload.conflictId,
      remainingConflicts: deps.candidateImportConflicts.value.length,
    };
  }

  return {
    runSidecarJobCrawl,
    runSidecarCandidateCrawl,
    runSidecarResumeCrawl,
    resolveCandidateImportConflict,
  };
}
