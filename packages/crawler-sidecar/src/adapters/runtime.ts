import os from "node:os";
import path from "node:path";

export interface AdapterRuntimeConfig {
  sessionRootDir: string;
  headless: boolean;
}

function envTrue(value: string | undefined): boolean {
  if (!value) {
    return false;
  }

  const normalized = value.trim().toLowerCase();
  return normalized === "1" || normalized === "true" || normalized === "yes";
}

export function resolveAdapterRuntimeConfig(): AdapterRuntimeConfig {
  const sessionRootDir = process.env.DOSS_SIDECAR_SESSION_DIR?.trim() ||
    path.join(os.homedir(), ".doss-recruiter", "sessions");

  const headless = !envTrue(process.env.DOSS_CRAWLER_HEADFUL);

  return {
    sessionRootDir,
    headless,
  };
}
