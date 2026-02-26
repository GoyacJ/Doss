import crypto from "node:crypto";
import express from "express";
import pino from "pino";
import { z } from "zod";
import type { CrawlMode, SourceType } from "@doss/shared";
import { getAdapter } from "./adapters";
import { CrawlTaskQueue } from "./task-queue";

const logger = pino({ name: "doss-crawler-sidecar" });

const sourceSchema = z.enum(["boss", "zhilian", "wuba"]);
const modeSchema = z.enum(["compliant", "advanced"]);

const jobParamsSchema = z.object({
  keyword: z.string().min(1),
  city: z.string().optional(),
  page: z.number().int().min(1).optional(),
});

const candidateParamsSchema = z.object({
  jobId: z.string().min(1),
  page: z.number().int().min(1).optional(),
});

const queue = new CrawlTaskQueue({
  maxRetries: 3,
  compliantDelayMs: 600,
  advancedDelayMs: 120,
});

function buildTaskFingerprint(source: SourceType, mode: CrawlMode, type: string, payload: unknown): string {
  return crypto
    .createHash("sha256")
    .update(JSON.stringify({ source, mode, type, payload }))
    .digest("hex");
}

export function createServer() {
  const app = express();
  app.use(express.json());

  app.get("/health", (_, response) => {
    response.json({ ok: true, service: "crawler-sidecar" });
  });

  app.post("/v1/session/check", async (request, response) => {
    const parsed = sourceSchema.safeParse(request.body?.source);
    if (!parsed.success) {
      response.status(400).json({ error: "Invalid source" });
      return;
    }

    const adapter = getAdapter(parsed.data);
    response.json(await adapter.checkSession());
  });

  app.post("/v1/crawl/jobs", async (request, response) => {
    const sourceParsed = sourceSchema.safeParse(request.body?.source);
    const modeParsed = modeSchema.safeParse(request.body?.mode);
    const paramsParsed = jobParamsSchema.safeParse(request.body?.params);

    if (!sourceParsed.success || !modeParsed.success || !paramsParsed.success) {
      response.status(400).json({ error: "Invalid crawl jobs request" });
      return;
    }

    const adapter = getAdapter(sourceParsed.data);
    const id = crypto.randomUUID();
    const result = await queue.enqueue({
      id,
      source: sourceParsed.data,
      mode: modeParsed.data,
      payload: paramsParsed.data,
      fingerprint: buildTaskFingerprint(sourceParsed.data, modeParsed.data, "jobs", paramsParsed.data),
      run: async () => adapter.crawlJobs(modeParsed.data, paramsParsed.data),
    });

    logger.info({ result }, "crawl jobs task completed");
    response.json(result);
  });

  app.post("/v1/crawl/candidates", async (request, response) => {
    const sourceParsed = sourceSchema.safeParse(request.body?.source);
    const modeParsed = modeSchema.safeParse(request.body?.mode);
    const paramsParsed = candidateParamsSchema.safeParse(request.body?.params);

    if (!sourceParsed.success || !modeParsed.success || !paramsParsed.success) {
      response.status(400).json({ error: "Invalid crawl candidates request" });
      return;
    }

    const adapter = getAdapter(sourceParsed.data);
    const id = crypto.randomUUID();
    const result = await queue.enqueue({
      id,
      source: sourceParsed.data,
      mode: modeParsed.data,
      payload: paramsParsed.data,
      fingerprint: buildTaskFingerprint(sourceParsed.data, modeParsed.data, "candidates", paramsParsed.data),
      run: async () => adapter.crawlCandidates(modeParsed.data, paramsParsed.data),
    });

    logger.info({ result }, "crawl candidates task completed");
    response.json(result);
  });

  app.post("/v1/crawl/resume", async (request, response) => {
    const sourceParsed = sourceSchema.safeParse(request.body?.source);
    const modeParsed = modeSchema.safeParse(request.body?.mode);
    const candidateIdParsed = z.string().min(1).safeParse(request.body?.candidateId);

    if (!sourceParsed.success || !modeParsed.success || !candidateIdParsed.success) {
      response.status(400).json({ error: "Invalid crawl resume request" });
      return;
    }

    const adapter = getAdapter(sourceParsed.data);
    const id = crypto.randomUUID();
    const result = await queue.enqueue({
      id,
      source: sourceParsed.data,
      mode: modeParsed.data,
      payload: { candidateId: candidateIdParsed.data },
      fingerprint: buildTaskFingerprint(sourceParsed.data, modeParsed.data, "resume", candidateIdParsed.data),
      run: async () => adapter.crawlResume(modeParsed.data, candidateIdParsed.data),
    });

    logger.info({ result }, "crawl resume task completed");
    response.json(result);
  });

  return app;
}
