import type { Ref } from "vue";
import type { InterviewQuestion, SourceType } from "@doss/shared";
import {
  type BackendAnalysisRecord,
  type HiringDecisionRecord,
  type InterviewEvaluationRecord,
  type InterviewFeedbackRecord,
  type InterviewKitRecord,
  type PipelineEvent,
  type ScoringResultRecord,
  type UpsertResumePayload,
} from "../../services/backend";
import type { UiAnalysisRecord } from "./types";

export interface AnalysisContextDeps {
  analyses: Ref<Record<number, UiAnalysisRecord[]>>;
  scoringResults: Ref<Record<number, ScoringResultRecord[]>>;
  interviewKits: Ref<Record<number, InterviewKitRecord | null>>;
  interviewFeedback: Ref<Record<number, InterviewFeedbackRecord[]>>;
  interviewEvaluations: Ref<Record<number, InterviewEvaluationRecord[]>>;
  hiringDecisions: Ref<Record<number, HiringDecisionRecord[]>>;
  pipelineEvents: Ref<Record<number, PipelineEvent[]>>;
  mapAnalysis: (record: BackendAnalysisRecord) => UiAnalysisRecord;
  runCandidateScoring: (input: { candidate_id: number; job_id?: number; run_id?: string }) => Promise<unknown>;
  listAnalysis: (candidateId: number) => Promise<BackendAnalysisRecord[]>;
  listPipelineEvents: (candidateId: number) => Promise<PipelineEvent[]>;
  listScoringResults: (candidateId: number) => Promise<ScoringResultRecord[]>;
  listInterviewEvaluations: (candidateId: number) => Promise<InterviewEvaluationRecord[]>;
  listHiringDecisions: (candidateId: number) => Promise<HiringDecisionRecord[]>;
  deleteResume: (candidateId: number) => Promise<boolean>;
  upsertResume: (input: UpsertResumePayload) => Promise<unknown>;
  refreshMetrics: () => Promise<void>;
  generateInterviewKit: (input: { candidate_id: number; job_id?: number }) => Promise<InterviewKitRecord>;
  saveInterviewKit: (input: {
    candidate_id: number;
    job_id?: number;
    questions: InterviewQuestion[];
  }) => Promise<InterviewKitRecord>;
  submitInterviewFeedback: (input: {
    candidate_id: number;
    job_id?: number;
    transcript_text: string;
    structured_feedback: Record<string, unknown>;
    recording_path?: string;
  }) => Promise<InterviewFeedbackRecord>;
  runInterviewEvaluation: (input: {
    candidate_id: number;
    job_id?: number;
    feedback_id?: number;
  }) => Promise<InterviewEvaluationRecord>;
}

export function mapBackendAnalysisRecord(record: BackendAnalysisRecord): UiAnalysisRecord {
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

export function createAnalysisContextModule(deps: AnalysisContextDeps) {
  async function saveResume(payload: {
    candidate_id: number;
    raw_text?: string;
    parsed?: Record<string, unknown>;
    enable_ocr?: boolean;
    job_id?: number;
    source?: SourceType;
    original_file?: {
      file_name: string;
      content_base64: string;
      content_type?: string;
    };
  }) {
    await deps.upsertResume({
      ...payload,
      source: payload.source ?? "manual",
    });
    await deps.runCandidateScoring({
      candidate_id: payload.candidate_id,
      job_id: payload.job_id,
    });
    const [latestScoringResults] = await Promise.all([
      deps.listScoringResults(payload.candidate_id),
      deps.refreshMetrics(),
    ]);
    deps.scoringResults.value[payload.candidate_id] = latestScoringResults;
  }

  async function analyzeCandidate(candidateId: number, jobId?: number, runId?: string) {
    await deps.runCandidateScoring({
      candidate_id: candidateId,
      job_id: jobId,
      ...(runId ? { run_id: runId } : {}),
    });
    deps.scoringResults.value[candidateId] = await deps.listScoringResults(candidateId);
    deps.analyses.value[candidateId] = (await deps.listAnalysis(candidateId)).map(deps.mapAnalysis);
  }

  async function loadCandidateContext(candidateId: number) {
    const [analysisData, eventData, scoringData, interviewEvaluationData, hiringDecisionData] = await Promise.all([
      deps.listAnalysis(candidateId),
      deps.listPipelineEvents(candidateId),
      deps.listScoringResults(candidateId),
      deps.listInterviewEvaluations(candidateId),
      deps.listHiringDecisions(candidateId),
    ]);
    deps.analyses.value[candidateId] = analysisData.map(deps.mapAnalysis);
    deps.pipelineEvents.value[candidateId] = eventData;
    deps.scoringResults.value[candidateId] = scoringData;
    deps.interviewEvaluations.value[candidateId] = interviewEvaluationData;
    deps.hiringDecisions.value[candidateId] = hiringDecisionData;
  }

  async function runScoring(candidateId: number, jobId?: number, runId?: string) {
    await deps.runCandidateScoring({
      candidate_id: candidateId,
      job_id: jobId,
      ...(runId ? { run_id: runId } : {}),
    });
    deps.scoringResults.value[candidateId] = await deps.listScoringResults(candidateId);
    return deps.scoringResults.value[candidateId];
  }

  async function rerunAiAnalysis(candidateId: number, jobId?: number, runId?: string) {
    await deps.runCandidateScoring({
      candidate_id: candidateId,
      job_id: jobId,
      ...(runId ? { run_id: runId } : {}),
    });
    await loadCandidateContext(candidateId);
    return deps.scoringResults.value[candidateId] ?? [];
  }

  async function importResumeFileAndAnalyze(payload: {
    candidateId: number;
    file: File;
    enableOcr?: boolean;
    jobId?: number;
  }): Promise<void> {
    const contentBase64 = await fileToBase64(payload.file);

    await saveResume({
      candidate_id: payload.candidateId,
      job_id: payload.jobId,
      enable_ocr: payload.enableOcr,
      original_file: {
        file_name: payload.file.name,
        content_base64: contentBase64,
        content_type: payload.file.type || undefined,
      },
    });
    await loadCandidateContext(payload.candidateId);
  }

  async function importResumeFile(payload: {
    candidateId: number;
    file: File;
    enableOcr?: boolean;
    jobId?: number;
  }): Promise<void> {
    const contentBase64 = await fileToBase64(payload.file);

    await deps.upsertResume({
      candidate_id: payload.candidateId,
      source: "manual",
      enable_ocr: payload.enableOcr,
      original_file: {
        file_name: payload.file.name,
        content_base64: contentBase64,
        content_type: payload.file.type || undefined,
      },
    });
    await deps.refreshMetrics();
  }

  async function removeResume(candidateId: number) {
    const removed = await deps.deleteResume(candidateId);
    await deps.refreshMetrics();
    return removed;
  }

  async function generateInterviewKit(candidateId: number, jobId?: number) {
    const kit = await deps.generateInterviewKit({
      candidate_id: candidateId,
      job_id: jobId,
    });
    deps.interviewKits.value[candidateId] = kit;
    return kit;
  }

  async function saveInterviewKit(payload: {
    candidate_id: number;
    job_id?: number;
    questions: InterviewQuestion[];
  }) {
    const kit = await deps.saveInterviewKit(payload);
    deps.interviewKits.value[payload.candidate_id] = kit;
    return kit;
  }

  async function submitInterviewFeedback(payload: {
    candidate_id: number;
    job_id?: number;
    transcript_text: string;
    structured_feedback: Record<string, unknown>;
    recording_path?: string;
  }) {
    const feedback = await deps.submitInterviewFeedback(payload);
    const existing = deps.interviewFeedback.value[payload.candidate_id] ?? [];
    deps.interviewFeedback.value[payload.candidate_id] = [feedback, ...existing];
    return feedback;
  }

  async function runInterviewEvaluation(payload: {
    candidate_id: number;
    job_id?: number;
    feedback_id?: number;
  }) {
    const evaluation = await deps.runInterviewEvaluation(payload);
    const existing = deps.interviewEvaluations.value[payload.candidate_id] ?? [];
    deps.interviewEvaluations.value[payload.candidate_id] = [evaluation, ...existing];
    return evaluation;
  }

  return {
    saveResume,
    analyzeCandidate,
    loadCandidateContext,
    runScoring,
    rerunAiAnalysis,
    importResumeFileAndAnalyze,
    importResumeFile,
    removeResume,
    generateInterviewKit,
    saveInterviewKit,
    submitInterviewFeedback,
    runInterviewEvaluation,
  };
}
