<script setup lang="ts">
import { computed } from "vue";
import { useRecruitingStore } from "../stores/recruiting";
import { formatStageLabel } from "../lib/pipeline";

const store = useRecruitingStore();

const metrics = computed(() => store.metrics);
</script>

<template>
  <section class="page">
    <header class="page-header">
      <h2>招聘总览</h2>
      <button class="button" @click="store.bootstrap">刷新</button>
    </header>

    <div class="grid-kpis" v-if="metrics">
      <article class="kpi-card">
        <p>职位总数</p>
        <h3>{{ metrics.total_jobs }}</h3>
      </article>
      <article class="kpi-card">
        <p>候选人总数</p>
        <h3>{{ metrics.total_candidates }}</h3>
      </article>
      <article class="kpi-card">
        <p>简历总数</p>
        <h3>{{ metrics.total_resumes }}</h3>
      </article>
      <article class="kpi-card">
        <p>待处理采集任务</p>
        <h3>{{ metrics.pending_tasks }}</h3>
      </article>
    </div>

    <article class="panel" v-if="metrics">
      <h3>阶段分布</h3>
      <div class="stage-list">
        <div v-for="item in store.stageSummary" :key="item.stage" class="stage-item">
          <span>{{ formatStageLabel(item.stage) }}</span>
          <strong>{{ item.count }}</strong>
        </div>
      </div>
    </article>

    <article class="panel" v-if="store.health">
      <h3>本地数据状态</h3>
      <p>数据库路径: {{ store.health.dbPath }}</p>
      <p>数据库存在: {{ store.health.dbExists ? "是" : "否" }}</p>
    </article>
  </section>
</template>
