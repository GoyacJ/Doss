<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { RouterLink, RouterView } from "vue-router";
import GlobalToastViewport from "./components/GlobalToastViewport.vue";
import UiBadge from "./components/UiBadge.vue";
import { sidecarHealthBadge } from "./lib/status";
import { useRecruitingStore } from "./stores/recruiting";
import { useToastStore } from "./stores/toast";

const store = useRecruitingStore();
const toast = useToastStore();
const navItems = [
  { to: "/dashboard", label: "总览" },
  { to: "/jobs", label: "职位池" },
  { to: "/candidates", label: "候选人" },
  { to: "/interview", label: "面试" },
  { to: "/decision", label: "决策" },
  { to: "/crawl", label: "采集任务" },
  { to: "/settings", label: "设置" },
] as const;
const sidecarBadge = computed(() => sidecarHealthBadge(store.sidecarHealthy));
const lastErrorToast = ref<string | null>(null);

onMounted(async () => {
  if (!store.hasBootstrapped) {
    try {
      await store.bootstrap();
    } catch (error) {
      const message = error instanceof Error ? error.message : "bootstrap_failed";
      toast.danger(`初始化失败：${message}`);
    }
  }
});

watch(
  () => store.lastError,
  (next) => {
    if (!next) {
      lastErrorToast.value = null;
      return;
    }
    if (next === lastErrorToast.value) {
      return;
    }
    toast.danger(next);
    lastErrorToast.value = next;
  },
);
</script>

<template>
  <div
    class="h-screen overflow-hidden grid grid-cols-[260px_1fr] lt-lg:min-h-screen lt-lg:h-auto lt-lg:overflow-visible lt-lg:grid-cols-1"
  >
    <aside
      class="h-screen overflow-y-auto p-5 border-r border-line backdrop-blur bg-sidebar lt-lg:h-auto lt-lg:overflow-visible lt-lg:border-r-0 lt-lg:border-b"
    >
      <h1 class="m-0 text-[1.45rem] tracking-[0.03em] font-700">Doss Recruiter</h1>
      <p class="my-1.5 mb-4 text-muted text-[0.92rem]">AI 辅助招聘工作台</p>

      <nav class="flex flex-col gap-2">
        <RouterLink
          v-for="item in navItems"
          :key="item.to"
          :to="item.to"
          active-class="border-brand/40 bg-brand/12"
          class="border border-transparent rounded-xl px-3 py-2.5 text-text no-underline transition-all duration-200 hover:border-line hover:bg-white/50"
        >
          {{ item.label }}
        </RouterLink>
      </nav>

      <div class="mt-4 border border-line rounded-xl px-3 py-2.5 bg-card">
        <p class="m-0 mb-1 text-muted text-[0.8rem]">Sidecar</p>
        <UiBadge :tone="sidecarBadge.tone">{{ sidecarBadge.label }}</UiBadge>
      </div>

    </aside>

    <main class="h-screen overflow-y-auto p-5 lt-lg:h-auto lt-lg:overflow-visible">
      <RouterView />
    </main>
    <GlobalToastViewport />
  </div>
</template>
