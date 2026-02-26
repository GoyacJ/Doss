import type { SourceType } from "@doss/shared";
import { BossAdapter } from "./boss";
import { LagouAdapter } from "./lagou";
import { resolveAdapterRuntimeConfig } from "./runtime";
import { WubaAdapter } from "./wuba";
import { ZhilianAdapter } from "./zhilian";
import type { SourceAdapter } from "./types";

const runtimeConfig = resolveAdapterRuntimeConfig();

const adapterMap: Partial<Record<SourceType, SourceAdapter>> = {
  boss: new BossAdapter({
    sessionDir: `${runtimeConfig.sessionRootDir}/boss`,
    headless: runtimeConfig.headless,
  }),
  zhilian: new ZhilianAdapter({
    sessionDir: `${runtimeConfig.sessionRootDir}/zhilian`,
    headless: runtimeConfig.headless,
  }),
  wuba: new WubaAdapter({
    sessionDir: `${runtimeConfig.sessionRootDir}/wuba`,
    headless: runtimeConfig.headless,
  }),
  lagou: new LagouAdapter({
    sessionDir: `${runtimeConfig.sessionRootDir}/lagou`,
    headless: runtimeConfig.headless,
  }),
};

export function getAdapter(source: SourceType): SourceAdapter {
  const adapter = adapterMap[source];
  if (!adapter) {
    throw new Error(`source_adapter_not_supported:${source}`);
  }
  return adapter;
}
