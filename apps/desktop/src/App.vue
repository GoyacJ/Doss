<script setup lang="ts">
import { onMounted } from "vue";
import { RouterLink, RouterView } from "vue-router";
import { useRecruitingStore } from "./stores/recruiting";

const store = useRecruitingStore();

onMounted(async () => {
  if (!store.hasBootstrapped) {
    await store.bootstrap();
  }
});
</script>

<template>
  <div class="shell">
    <aside class="sidebar">
      <h1 class="brand">Doss Recruiter</h1>
      <p class="subtitle">AI 辅助招聘工作台</p>

      <nav class="nav">
        <RouterLink to="/dashboard" class="nav-link">总览</RouterLink>
        <RouterLink to="/jobs" class="nav-link">职位池</RouterLink>
        <RouterLink to="/candidates" class="nav-link">候选人</RouterLink>
        <RouterLink to="/crawl" class="nav-link">采集任务</RouterLink>
        <RouterLink to="/settings" class="nav-link">设置</RouterLink>
      </nav>

      <div class="status-box">
        <p>Sidecar</p>
        <strong :class="store.sidecarHealthy ? 'ok' : 'bad'">
          {{ store.sidecarHealthy ? "在线" : "离线" }}
        </strong>
      </div>

      <div v-if="store.lastError" class="error-box">
        {{ store.lastError }}
      </div>
    </aside>

    <main class="main-content">
      <RouterView />
    </main>
  </div>
</template>
