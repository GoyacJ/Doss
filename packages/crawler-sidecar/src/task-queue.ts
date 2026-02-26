import type { CrawlMode, SourceType } from "@doss/shared";

export interface QueueTask {
  id: string;
  source: SourceType;
  mode: CrawlMode;
  fingerprint: string;
  payload: Record<string, unknown>;
  run: () => Promise<unknown>;
}

export interface QueueOptions {
  maxRetries: number;
  compliantDelayMs: number;
  advancedDelayMs: number;
}

export interface QueueResult {
  id: string;
  source: SourceType;
  mode: CrawlMode;
  status: "SUCCEEDED" | "FAILED" | "SKIPPED_DUPLICATE";
  attempts: number;
  output?: unknown;
  error?: string;
}

function sleep(delayMs: number): Promise<void> {
  if (delayMs <= 0) {
    return Promise.resolve();
  }

  return new Promise((resolve) => setTimeout(resolve, delayMs));
}

export class CrawlTaskQueue {
  private readonly maxRetries: number;
  private readonly compliantDelayMs: number;
  private readonly advancedDelayMs: number;
  private readonly inFlightByFingerprint = new Map<string, Promise<QueueResult>>();

  constructor(options: QueueOptions) {
    this.maxRetries = Math.max(1, options.maxRetries);
    this.compliantDelayMs = Math.max(0, options.compliantDelayMs);
    this.advancedDelayMs = Math.max(0, options.advancedDelayMs);
  }

  async enqueue(task: QueueTask): Promise<QueueResult> {
    const existing = this.inFlightByFingerprint.get(task.fingerprint);
    if (existing) {
      return {
        id: task.id,
        source: task.source,
        mode: task.mode,
        status: "SKIPPED_DUPLICATE",
        attempts: 0,
      };
    }

    const execution = this.execute(task).finally(() => {
      this.inFlightByFingerprint.delete(task.fingerprint);
    });

    this.inFlightByFingerprint.set(task.fingerprint, execution);
    return execution;
  }

  private async execute(task: QueueTask): Promise<QueueResult> {
    const delayMs = task.mode === "compliant" ? this.compliantDelayMs : this.advancedDelayMs;

    let attempts = 0;
    let lastError: unknown = undefined;

    while (attempts < this.maxRetries) {
      attempts += 1;
      await sleep(delayMs);
      try {
        const output = await task.run();
        return {
          id: task.id,
          source: task.source,
          mode: task.mode,
          status: "SUCCEEDED",
          attempts,
          output,
        };
      } catch (error) {
        lastError = error;
      }
    }

    return {
      id: task.id,
      source: task.source,
      mode: task.mode,
      status: "FAILED",
      attempts,
      error: lastError instanceof Error ? lastError.message : "Unknown queue error",
    };
  }
}
