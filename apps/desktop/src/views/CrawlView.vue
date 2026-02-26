<script setup lang="ts">
import { reactive, ref } from "vue";
import type { CrawlMode } from "@doss/shared";
import { useRecruitingStore } from "../stores/recruiting";

const store = useRecruitingStore();

const taskForm = reactive({
  source: "boss" as "boss" | "zhilian" | "wuba",
  mode: "compliant" as CrawlMode,
  task_type: "crawl_jobs",
  keyword: "前端",
  city: "上海",
});

const sidecarResult = ref<string>("");
const importedCount = ref<number | null>(null);
const importedCandidateCount = ref<number | null>(null);
const autoResumeCount = ref<number | null>(null);
const autoAnalysisCount = ref<number | null>(null);
const autoErrorCount = ref<number | null>(null);
const candidateTaskForm = reactive({
  source: "boss" as "boss" | "zhilian" | "wuba",
  mode: "compliant" as CrawlMode,
  localJobId: 0,
  externalJobId: "",
});

async function createTask() {
  await store.addCrawlTask({
    source: taskForm.source,
    mode: taskForm.mode,
    task_type: taskForm.task_type,
    payload: {
      keyword: taskForm.keyword,
      city: taskForm.city,
    },
  });
}

async function runSidecar() {
  const response = await store.runSidecarJobCrawl({
    source: taskForm.source,
    mode: taskForm.mode,
    keyword: taskForm.keyword,
    city: taskForm.city,
  });

  importedCount.value = response.importedJobs;
  sidecarResult.value = JSON.stringify(response.result, null, 2);
}

async function runCandidateSidecar() {
  if (!candidateTaskForm.localJobId) {
    return;
  }

  const response = await store.runSidecarCandidateCrawl({
    source: candidateTaskForm.source,
    mode: candidateTaskForm.mode,
    localJobId: Number(candidateTaskForm.localJobId),
    externalJobId: candidateTaskForm.externalJobId || undefined,
  });

  importedCandidateCount.value = response.importedCandidates;
  autoResumeCount.value = response.resumeAutoProcessed;
  autoAnalysisCount.value = response.analysisTriggered;
  autoErrorCount.value = response.autoProcessErrors.length;
  sidecarResult.value = JSON.stringify(response.result, null, 2);
}
</script>

<template>
  <section class="page">
    <header class="page-header">
      <h2>采集任务</h2>
      <button class="button" @click="store.refreshTasks">刷新任务</button>
    </header>

    <article class="panel">
      <h3>新建采集任务</h3>
      <div class="form-grid">
        <select v-model="taskForm.source">
          <option value="boss">Boss</option>
          <option value="zhilian">智联</option>
          <option value="wuba">58</option>
        </select>
        <select v-model="taskForm.mode">
          <option value="compliant">合规模式</option>
          <option value="advanced">高级模式</option>
        </select>
        <input v-model="taskForm.task_type" placeholder="任务类型" />
        <input v-model="taskForm.keyword" placeholder="关键词" />
        <input v-model="taskForm.city" placeholder="城市" />
      </div>
      <div class="inline-row">
        <button class="button" @click="createTask">写入本地任务</button>
        <button class="button secondary" @click="runSidecar">执行抓取闭环（Sidecar→入库）</button>
      </div>
      <p v-if="importedCount !== null">本次导入职位数: {{ importedCount }}</p>
      <pre v-if="sidecarResult" class="code-box">{{ sidecarResult }}</pre>
    </article>

    <article class="panel">
      <h3>抓取候选人并入库</h3>
      <div class="form-grid">
        <select v-model="candidateTaskForm.source">
          <option value="boss">Boss</option>
          <option value="zhilian">智联</option>
          <option value="wuba">58</option>
        </select>
        <select v-model="candidateTaskForm.mode">
          <option value="compliant">合规模式</option>
          <option value="advanced">高级模式</option>
        </select>
        <select v-model.number="candidateTaskForm.localJobId">
          <option :value="0">选择本地职位</option>
          <option v-for="job in store.jobs" :key="job.id" :value="job.id">
            #{{ job.id }} {{ job.title }} / {{ job.company }}
          </option>
        </select>
        <input v-model="candidateTaskForm.externalJobId" placeholder="外部职位ID（可选）" />
      </div>
      <div class="inline-row">
        <button class="button secondary" @click="runCandidateSidecar">
          执行候选人闭环（Sidecar→入库）
        </button>
      </div>
      <p v-if="importedCandidateCount !== null">本次导入候选人数: {{ importedCandidateCount }}</p>
      <p v-if="autoResumeCount !== null">自动抓取简历数: {{ autoResumeCount }}</p>
      <p v-if="autoAnalysisCount !== null">自动触发分析数: {{ autoAnalysisCount }}</p>
      <p v-if="autoErrorCount !== null">自动处理失败数: {{ autoErrorCount }}</p>
    </article>

    <article class="panel">
      <h3>任务列表</h3>
      <table class="table">
        <thead>
          <tr>
            <th>ID</th>
            <th>来源</th>
            <th>模式</th>
            <th>类型</th>
            <th>状态</th>
            <th>重试</th>
            <th>更新时间</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="task in store.tasks" :key="task.id">
            <td>#{{ task.id }}</td>
            <td>{{ task.source }}</td>
            <td>{{ task.mode }}</td>
            <td>{{ task.task_type }}</td>
            <td>{{ task.status }}</td>
            <td>{{ task.retry_count }}</td>
            <td>{{ task.updated_at }}</td>
          </tr>
        </tbody>
      </table>
    </article>
  </section>
</template>
