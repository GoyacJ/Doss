<script setup lang="ts">
import { computed } from "vue";
import { useRecruitingStore } from "../stores/recruiting";
import { formatStageLabel } from "../lib/pipeline";
import { stageTone } from "../lib/status";
import UiBadge from "../components/UiBadge.vue";
import UiButton from "../components/UiButton.vue";
import UiInfoRow from "../components/UiInfoRow.vue";
import UiMetricCard from "../components/UiMetricCard.vue";
import UiPanel from "../components/UiPanel.vue";

const store = useRecruitingStore();

const metrics = computed(() => store.metrics);
</script>

<template>
  <section class="flex flex-col gap-4">
    <header class="flex items-center justify-between gap-3">
      <h2 class="text-2xl font-700">招聘总览</h2>
      <UiButton @click="store.bootstrap">刷新</UiButton>
    </header>

    <div v-if="metrics" class="grid grid-cols-4 lt-lg:grid-cols-2 gap-3">
      <UiMetricCard label="职位总数" :value="metrics.total_jobs" />
      <UiMetricCard label="候选人总数" :value="metrics.total_candidates" />
      <UiMetricCard label="简历总数" :value="metrics.total_resumes" />
      <UiMetricCard label="待处理采集任务" :value="metrics.pending_tasks" />
    </div>

    <UiPanel v-if="metrics" title="阶段分布">
      <div class="grid grid-cols-3 lt-lg:grid-cols-2 gap-2.5">
        <div v-for="item in store.stageSummary" :key="item.stage" class="border border-line rounded-xl px-2.5 py-2.5 flex justify-between">
          <UiBadge :tone="stageTone(item.stage)">{{ formatStageLabel(item.stage) }}</UiBadge>
          <strong>{{ item.count }}</strong>
        </div>
      </div>
    </UiPanel>

    <UiPanel v-if="store.health" title="本地数据状态">
      <div class="flex flex-col gap-1.5">
        <UiInfoRow label="数据库路径" :value="store.health.dbPath" />
        <UiInfoRow label="数据库存在" :value="store.health.dbExists ? '是' : '否'" />
      </div>
    </UiPanel>
  </section>
</template>
