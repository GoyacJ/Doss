import type { Ref } from "vue";
import type { CandidateRecord, CrawlMode, JobRecord } from "@doss/shared";
import {
  extractCandidateImportItems,
  extractJobImportItems,
  type CandidateImportItem,
} from "../../lib/crawl-import";
import type { SidecarQueueResult } from "../../services/backend";
import type {
  AutoProcessTarget,
  CandidateImportBatchResult,
  CandidateImportConflict,
  CandidateImportSource,
  TaskPersonSyncResult,
} from "./types";

function normalizeText(value: string | null | undefined): string {
  return (value ?? "")
    .trim()
    .toLowerCase()
    .replace(/\s+/g, " ");
}

function buildCandidateNameKey(name: string): string {
  return normalizeText(name);
}

function matchesDedupeRule(existing: CandidateRecord, incoming: CandidateImportItem): boolean {
  if (buildCandidateNameKey(existing.name) !== buildCandidateNameKey(incoming.name)) {
    return false;
  }

  if (
    typeof existing.age === "number"
    && typeof incoming.age === "number"
    && existing.age !== incoming.age
  ) {
    return false;
  }

  const existingAddress = normalizeText(existing.address);
  const incomingAddress = normalizeText(incoming.address);
  if (existingAddress && incomingAddress && existingAddress !== incomingAddress) {
    return false;
  }

  return true;
}

function replaceCandidateInStore(candidates: Ref<CandidateRecord[]>, updated: CandidateRecord) {
  const index = candidates.value.findIndex((item) => item.id === updated.id);
  if (index >= 0) {
    candidates.value[index] = updated;
  } else {
    candidates.value.unshift(updated);
  }
}

export interface CandidateImportModuleDeps {
  jobs: Ref<JobRecord[]>;
  candidates: Ref<CandidateRecord[]>;
  candidateImportConflicts: Ref<CandidateImportConflict[]>;
  createJob: (payload: {
    source: CandidateImportSource;
    external_id?: string;
    title: string;
    company: string;
    city?: string;
    salary_k?: string;
    description?: string;
  }) => Promise<JobRecord>;
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
}

export function createCandidateImportModule(deps: CandidateImportModuleDeps) {
  async function importJobsFromSidecarResult(
    result: SidecarQueueResult,
    source: CandidateImportSource,
  ): Promise<JobRecord[]> {
    const importItems = extractJobImportItems(result);
    if (importItems.length === 0) {
      return [];
    }

    const existingByExternalId = new Set(
      deps.jobs.value
        .map((item) => item.external_id)
        .filter((item): item is string => Boolean(item)),
    );
    const existingByIdentity = new Set(
      deps.jobs.value.map((item) => `${item.source}:${item.title}:${item.company}:${item.city ?? ""}`),
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

      const created = await deps.createJob({
        source,
        external_id: item.external_id,
        title: item.title,
        company: item.company,
        city: item.city,
        salary_k: item.salary_k,
        description: item.description,
      });
      inserted.push(created);
      deps.jobs.value.unshift(created);
      if (created.external_id) {
        existingByExternalId.add(created.external_id);
      }
      existingByIdentity.add(identity);
    }

    return inserted;
  }

  async function importCandidatesFromItems(
    importItems: CandidateImportItem[],
    source: CandidateImportSource,
    mode: CrawlMode,
    localJobId: number,
  ): Promise<CandidateImportBatchResult> {
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
      deps.candidates.value
        .map((item) => item.external_id)
        .filter((item): item is string => Boolean(item)),
    );
    const existingByName = new Map<string, CandidateRecord[]>();
    for (const candidate of deps.candidates.value) {
      const key = buildCandidateNameKey(candidate.name);
      const list = existingByName.get(key) ?? [];
      list.push(candidate);
      existingByName.set(key, list);
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

      const nameKey = buildCandidateNameKey(item.name);
      const nameMatches = existingByName.get(nameKey) ?? [];
      const identityMatches = nameMatches.filter((candidate) => matchesDedupeRule(candidate, item));

      if (identityMatches.length === 1) {
        const target = identityMatches[0];
        const mergedRecord = await deps.mergeCandidateImport({
          candidate_id: target.id,
          current_company: item.current_company,
          years_of_experience: item.years_of_experience,
          address: item.address,
          tags: mergeTag ? [...item.tags, mergeTag] : item.tags,
          phone: item.phone,
          email: item.email,
          job_id: localJobId,
        });
        replaceCandidateInStore(deps.candidates, mergedRecord);
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
          id: `multi-${nameKey}-${Date.now()}-${conflicts.length}`,
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

      const created = await deps.createCandidate({
        source,
        external_id: item.external_id,
        name: item.name,
        current_company: item.current_company,
        years_of_experience: item.years_of_experience,
        age: item.age,
        address: item.address,
        tags: item.tags,
        phone: item.phone,
        email: item.email,
        job_id: localJobId,
      });
      inserted.push(created);
      deps.candidates.value.unshift(created);
      if (created.external_id) {
        existingByExternalId.add(created.external_id);
      }
      if (created.external_id) {
        autoProcessTargets.push({
          localCandidateId: created.id,
          externalCandidateId: created.external_id,
        });
      }
      const list = existingByName.get(nameKey) ?? [];
      list.push(created);
      existingByName.set(nameKey, list);
    }

    if (conflicts.length > 0) {
      deps.candidateImportConflicts.value = [...conflicts, ...deps.candidateImportConflicts.value];
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

  async function importCandidatesFromSidecarResult(
    result: SidecarQueueResult,
    source: CandidateImportSource,
    mode: CrawlMode,
    localJobId: number,
  ): Promise<CandidateImportBatchResult> {
    return importCandidatesFromItems(
      extractCandidateImportItems(result),
      source,
      mode,
      localJobId,
    );
  }

  async function importSingleCandidateItem(payload: {
    item: CandidateImportItem;
    source: CandidateImportSource;
    mode: CrawlMode;
    localJobId: number;
  }): Promise<TaskPersonSyncResult> {
    const result = await importCandidatesFromItems(
      [payload.item],
      payload.source,
      payload.mode,
      payload.localJobId,
    );

    if (result.conflicts.length > 0) {
      return {
        status: "FAILED",
        reason: result.conflicts[0]?.reasons.join(",") || "candidate_sync_conflict",
      };
    }

    if (result.importedCandidates[0]) {
      return {
        status: "SYNCED",
        candidateId: result.importedCandidates[0].id,
      };
    }

    if (result.mergedCandidates[0]) {
      return {
        status: "SYNCED",
        candidateId: result.mergedCandidates[0].id,
      };
    }

    if (result.skippedRows > 0) {
      const existed = payload.item.external_id
        ? deps.candidates.value.find((item) => item.external_id === payload.item.external_id)
        : undefined;
      return {
        status: "SYNCED",
        candidateId: existed?.id,
      };
    }

    return {
      status: "FAILED",
      reason: "candidate_sync_noop",
    };
  }

  return {
    importJobsFromSidecarResult,
    importCandidatesFromSidecarResult,
    importSingleCandidateItem,
  };
}
