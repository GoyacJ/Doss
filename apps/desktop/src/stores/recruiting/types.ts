import type { AnalysisResult, CandidateRecord, CrawlMode } from "@doss/shared";
import type { CandidateImportItem } from "../../lib/crawl-import";

export type UiAnalysisRecord = AnalysisResult & { id: number; createdAt: string };

export type CandidateImportSource = "boss" | "zhilian" | "wuba" | "lagou";
export type CrawlTaskSource = CandidateImportSource | "all";
export const CRAWL_PLATFORM_SOURCES: CandidateImportSource[] = ["boss", "zhilian", "wuba", "lagou"];

export type ConflictResolutionAction = "merge" | "create" | "skip";

export type AutoProcessTarget = {
  localCandidateId: number;
  externalCandidateId: string;
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

export type CandidateImportBatchResult = {
  fetchedRows: number;
  importedCandidates: CandidateRecord[];
  mergedCandidates: CandidateRecord[];
  conflicts: CandidateImportConflict[];
  skippedRows: number;
  autoProcessTargets: AutoProcessTarget[];
};

export type SidecarTaskError = Error & {
  sidecarErrorCode?: string;
  sidecarSnapshot?: Record<string, unknown>;
};

export interface CrawlCandidatesTaskPayload {
  localJobId: number;
  localJobTitle: string;
  localJobCity?: string;
  batchSize: number;
  scheduleType: "ONCE" | "DAILY" | "MONTHLY";
  scheduleTime?: string;
  scheduleDay?: number;
  retryCount: number;
  retryBackoffMs: number;
  autoSyncToCandidates: boolean;
}

export interface TaskPersonSyncResult {
  status: "SYNCED" | "FAILED";
  candidateId?: number;
  reason?: string;
}
