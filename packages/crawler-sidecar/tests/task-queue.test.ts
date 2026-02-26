import { describe, expect, it } from "vitest";
import { CrawlTaskQueue, type QueueTask } from "../src/task-queue";

describe("CrawlTaskQueue", () => {
  it("retries failed tasks up to max retries", async () => {
    let attempts = 0;
    const task: QueueTask = {
      id: "t1",
      source: "boss",
      mode: "compliant",
      fingerprint: "fp-1",
      payload: {},
      run: async () => {
        attempts += 1;
        if (attempts < 3) {
          throw new Error("temporary failure");
        }
        return { ok: true };
      },
    };

    const queue = new CrawlTaskQueue({ maxRetries: 3, compliantDelayMs: 0, advancedDelayMs: 0 });
    const result = await queue.enqueue(task);

    expect(result.status).toBe("SUCCEEDED");
    expect(attempts).toBe(3);
  });

  it("deduplicates by fingerprint while pending/running", async () => {
    const queue = new CrawlTaskQueue({ maxRetries: 1, compliantDelayMs: 0, advancedDelayMs: 0 });

    let firstStarted = false;
    const longTask: QueueTask = {
      id: "t2",
      source: "zhilian",
      mode: "compliant",
      fingerprint: "same-fp",
      payload: {},
      run: async () => {
        firstStarted = true;
        await new Promise((resolve) => setTimeout(resolve, 10));
        return { ok: true };
      },
    };

    const duplicate: QueueTask = {
      ...longTask,
      id: "t3",
    };

    const p1 = queue.enqueue(longTask);
    if (!firstStarted) {
      await new Promise((resolve) => setTimeout(resolve, 1));
    }
    const p2 = queue.enqueue(duplicate);

    const [, second] = await Promise.all([p1, p2]);

    expect(second.status).toBe("SKIPPED_DUPLICATE");
  });
});
