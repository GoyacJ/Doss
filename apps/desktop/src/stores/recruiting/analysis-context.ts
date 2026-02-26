import type { Ref } from "vue";
import type { InterviewQuestion, SourceType } from "@doss/shared";
import {
  type BackendAnalysisRecord,
  type HiringDecisionRecord,
  type InterviewEvaluationRecord,
  type InterviewFeedbackRecord,
  type InterviewKitRecord,
  type ParsedResumeFile,
  type PipelineEvent,
  type ScreeningResultRecord,
  type UpsertResumePayload,
} from "../../services/backend";
import type { UiAnalysisRecord } from "./types";

export interface AnalysisContextDeps {
  analyses: Ref<Record<number, UiAnalysisRecord[]>>;
  screeningResults: Ref<Record<number, ScreeningResultRecord[]>>;
  interviewKits: Ref<Record<number, InterviewKitRecord | null>>;
  interviewFeedback: Ref<Record<number, InterviewFeedbackRecord[]>>;
  interviewEvaluations: Ref<Record<number, InterviewEvaluationRecord[]>>;
  hiringDecisions: Ref<Record<number, HiringDecisionRecord[]>>;
  pipelineEvents: Ref<Record<number, PipelineEvent[]>>;
  mapAnalysis: (record: BackendAnalysisRecord) => UiAnalysisRecord;
  runCandidateAnalysis: (input: { candidate_id: number; job_id?: number }) => Promise<unknown>;
  listAnalysis: (candidateId: number) => Promise<BackendAnalysisRecord[]>;
  listPipelineEvents: (candidateId: number) => Promise<PipelineEvent[]>;
  listScreeningResults: (candidateId: number) => Promise<ScreeningResultRecord[]>;
  listInterviewEvaluations: (candidateId: number) => Promise<InterviewEvaluationRecord[]>;
  listHiringDecisions: (candidateId: number) => Promise<HiringDecisionRecord[]>;
  runResumeScreening: (input: { candidate_id: number; job_id?: number }) => Promise<ScreeningResultRecord>;
  upsertResume: (input: UpsertResumePayload) => Promise<unknown>;
  refreshMetrics: () => Promise<void>;
  parseResumeFile: (input: {
    file_name: string;
    content_base64: string;
    enable_ocr?: boolean;
  }) => Promise<ParsedResumeFile>;
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
    raw_text: string;
    parsed: Record<string, unknown>;
    job_id?: number;
    source?: SourceType;
  }) {
    await deps.upsertResume({
      ...payload,
      source: payload.source ?? "manual",
    });
    await deps.runResumeScreening({
      candidate_id: payload.candidate_id,
      job_id: payload.job_id,
    });
    const [latestScreeningResults] = await Promise.all([
      deps.listScreeningResults(payload.candidate_id),
      deps.refreshMetrics(),
    ]);
    deps.screeningResults.value[payload.candidate_id] = latestScreeningResults;
  }

  async function analyzeCandidate(candidateId: number, jobId?: number) {
    await deps.runCandidateAnalysis({
      candidate_id: candidateId,
      job_id: jobId,
    });
    deps.analyses.value[candidateId] = (await deps.listAnalysis(candidateId)).map(deps.mapAnalysis);
  }

  async function loadCandidateContext(candidateId: number) {
    const [analysisData, eventData, screeningData, interviewEvaluationData, hiringDecisionData] = await Promise.all([
      deps.listAnalysis(candidateId),
      deps.listPipelineEvents(candidateId),
      deps.listScreeningResults(candidateId),
      deps.listInterviewEvaluations(candidateId),
      deps.listHiringDecisions(candidateId),
    ]);
    deps.analyses.value[candidateId] = analysisData.map(deps.mapAnalysis);
    deps.pipelineEvents.value[candidateId] = eventData;
    deps.screeningResults.value[candidateId] = screeningData;
    deps.interviewEvaluations.value[candidateId] = interviewEvaluationData;
    deps.hiringDecisions.value[candidateId] = hiringDecisionData;
  }

  async function runScreening(candidateId: number, jobId?: number) {
    await deps.runResumeScreening({
      candidate_id: candidateId,
      job_id: jobId,
    });
    deps.screeningResults.value[candidateId] = await deps.listScreeningResults(candidateId);
    return deps.screeningResults.value[candidateId];
  }

  async function importResumeFileAndAnalyze(payload: {
    candidateId: number;
    file: File;
    enableOcr?: boolean;
    jobId?: number;
  }): Promise<ParsedResumeFile> {
    const contentBase64 = await fileToBase64(payload.file);
    const parsedFile = await deps.parseResumeFile({
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
    runScreening,
    importResumeFileAndAnalyze,
    generateInterviewKit,
    saveInterviewKit,
    submitInterviewFeedback,
    runInterviewEvaluation,
  };
}
