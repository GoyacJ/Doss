<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";
import type { CrawlMode } from "@doss/shared";
import { useRecruitingStore } from "../stores/recruiting";
import { taskStatusLabel, taskStatusTone } from "../lib/status";
import UiBadge from "../components/UiBadge.vue";
import UiButton from "../components/UiButton.vue";
import UiField from "../components/UiField.vue";
import UiInfoRow from "../components/UiInfoRow.vue";
import UiPanel from "../components/UiPanel.vue";
import UiTable from "../components/UiTable.vue";
import UiTd from "../components/UiTd.vue";
import UiTh from "../components/UiTh.vue";
import { useToastStore } from "../stores/toast";

const store = useRecruitingStore();
const toast = useToastStore();

const taskForm = reactive({
  source: "boss" as "boss" | "zhilian" | "wuba" | "lagou",
  mode: "compliant" as CrawlMode,
  task_type: "crawl_jobs",
  keyword: "前端",
  city: "上海",
});

const sidecarResult = ref<string>("");
const taskSettingsForm = reactive({
  auto_batch_concurrency: 2,
  auto_retry_count: 1,
  auto_retry_backoff_ms: 450,
});
const candidateTaskForm = reactive({
  source: "boss" as "boss" | "zhilian" | "wuba" | "lagou",
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
  toast.success("已写入本地采集任务");
}

async function runSidecar() {
  const response = await store.runSidecarJobCrawl({
    source: taskForm.source,
    mode: taskForm.mode,
    keyword: taskForm.keyword,
    city: taskForm.city,
  });

  sidecarResult.value = JSON.stringify(response.result, null, 2);
  toast.success(`职位抓取完成，本次导入 ${response.importedJobs} 条`);
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

  sidecarResult.value = JSON.stringify(response.result, null, 2);
  toast.success(
    `候选人抓取完成：导入 ${response.importedCandidates}，归并 ${response.mergedCandidates}，冲突 ${response.conflictCandidates}，自动抓简历 ${response.resumeAutoProcessed}，自动分析 ${response.analysisTriggered}`,
    6000,
  );
  if (response.autoProcessErrors.length > 0) {
    toast.warning(`自动处理失败 ${response.autoProcessErrors.length} 条，请查看导入质量报告`, 5200);
  }
}

async function loadTaskSettings() {
  const settings = await store.loadTaskSettings();
  taskSettingsForm.auto_batch_concurrency = settings.auto_batch_concurrency;
  taskSettingsForm.auto_retry_count = settings.auto_retry_count;
  taskSettingsForm.auto_retry_backoff_ms = settings.auto_retry_backoff_ms;
}

async function saveTaskSettings() {
  await store.saveTaskSettings({
    auto_batch_concurrency: Number(taskSettingsForm.auto_batch_concurrency),
    auto_retry_count: Number(taskSettingsForm.auto_retry_count),
    auto_retry_backoff_ms: Number(taskSettingsForm.auto_retry_backoff_ms),
  });
  toast.success("任务运行参数已保存");
}

async function pauseTask(taskId: number) {
  await store.pauseTask(taskId);
}

async function resumeTask(taskId: number) {
  await store.resumeTask(taskId);
}

async function cancelTask(taskId: number) {
  await store.cancelTask(taskId);
}

async function resolveConflict(
  conflictId: string,
  action: "merge" | "create" | "skip",
) {
  await store.resolveCandidateImportConflict({
    conflictId,
    action,
  });
  toast.success("冲突项已处理");
}

onMounted(() => {
  loadTaskSettings().catch(() => {
    toast.warning("加载任务参数失败");
  });
});
</script>

<template>
  <section class="flex flex-col gap-4">
    <header class="flex items-center justify-between gap-3">
      <h2 class="text-2xl font-700">采集任务</h2>
      <UiButton @click="store.refreshTasks">刷新任务</UiButton>
    </header>

    <UiPanel title="任务运行参数">
      <div class="grid grid-cols-2 gap-2.5 mb-2.5 lt-lg:grid-cols-1">
        <UiField label="自动批处理并发">
          <input v-model.number="taskSettingsForm.auto_batch_concurrency" type="number" min="1" max="8" step="1" />
        </UiField>
        <UiField label="失败重试次数">
          <input v-model.number="taskSettingsForm.auto_retry_count" type="number" min="0" max="6" step="1" />
        </UiField>
        <UiField label="重试退避(ms)">
          <input v-model.number="taskSettingsForm.auto_retry_backoff_ms" type="number" min="100" max="8000" step="50" />
        </UiField>
      </div>
      <div class="flex items-center gap-2.5 mb-2.5 flex-wrap">
        <UiButton variant="secondary" @click="loadTaskSettings">重新加载</UiButton>
        <UiButton @click="saveTaskSettings">保存任务参数</UiButton>
      </div>
    </UiPanel>

    <UiPanel title="新建采集任务">
      <div class="grid grid-cols-2 gap-2.5 mb-2.5 lt-lg:grid-cols-1">
        <UiField label="来源平台">
          <select v-model="taskForm.source">
            <option value="boss">Boss</option>
            <option value="zhilian">智联</option>
            <option value="wuba">58</option>
            <option value="lagou">拉勾</option>
          </select>
        </UiField>
        <UiField label="采集模式">
          <select v-model="taskForm.mode">
            <option value="compliant">合规模式</option>
            <option value="advanced">高级模式</option>
          </select>
        </UiField>
        <UiField label="任务类型">
          <input v-model="taskForm.task_type" placeholder="任务类型" />
        </UiField>
        <UiField label="关键词">
          <input v-model="taskForm.keyword" placeholder="例如：前端工程师" />
        </UiField>
        <UiField label="城市">
          <input v-model="taskForm.city" placeholder="例如：上海" />
        </UiField>
      </div>
      <div class="flex items-center gap-2.5 mb-2.5 flex-wrap">
        <UiButton @click="createTask">写入本地任务</UiButton>
        <UiButton variant="secondary" @click="runSidecar">执行抓取闭环（Sidecar→入库）</UiButton>
      </div>
      <pre v-if="sidecarResult" class="border border-line rounded-xl p-2.5 bg-code overflow-x-auto">{{ sidecarResult }}</pre>
    </UiPanel>

    <UiPanel title="抓取候选人并入库">
      <div class="grid grid-cols-2 gap-2.5 mb-2.5 lt-lg:grid-cols-1">
        <UiField label="来源平台">
          <select v-model="candidateTaskForm.source">
            <option value="boss">Boss</option>
            <option value="zhilian">智联</option>
            <option value="wuba">58</option>
            <option value="lagou">拉勾</option>
          </select>
        </UiField>
        <UiField label="采集模式">
          <select v-model="candidateTaskForm.mode">
            <option value="compliant">合规模式</option>
            <option value="advanced">高级模式</option>
          </select>
        </UiField>
        <UiField label="本地职位">
          <select v-model.number="candidateTaskForm.localJobId">
            <option :value="0">选择本地职位</option>
            <option v-for="job in store.jobs" :key="job.id" :value="job.id">
              #{{ job.id }} {{ job.title }} / {{ job.company }}
            </option>
          </select>
        </UiField>
        <UiField label="外部职位ID（可选）">
          <input v-model="candidateTaskForm.externalJobId" placeholder="平台职位ID" />
        </UiField>
      </div>
      <div class="flex items-center gap-2.5 mb-2.5 flex-wrap">
        <UiButton variant="secondary" @click="runCandidateSidecar">执行候选人闭环（Sidecar→入库）</UiButton>
      </div>
    </UiPanel>

    <UiPanel v-if="store.lastCandidateImportReport" title="导入质量报告">
      <div class="flex flex-col gap-1.5">
        <UiInfoRow label="生成时间" :value="store.lastCandidateImportReport.generatedAt" />
        <UiInfoRow label="来源" :value="`${store.lastCandidateImportReport.source} / 职位ID: ${store.lastCandidateImportReport.localJobId}`" />
        <UiInfoRow label="抓取条数" :value="store.lastCandidateImportReport.fetchedRows" />
        <UiInfoRow label="新建" :value="store.lastCandidateImportReport.importedRows" />
        <UiInfoRow label="归并" :value="store.lastCandidateImportReport.mergedRows" />
        <UiInfoRow label="冲突待确认" :value="store.lastCandidateImportReport.conflictRows" />
        <UiInfoRow label="跳过重复" :value="store.lastCandidateImportReport.skippedRows" />
        <UiInfoRow label="自动抓简历" :value="store.lastCandidateImportReport.autoResumeProcessed" />
        <UiInfoRow label="自动分析" :value="store.lastCandidateImportReport.autoAnalysisTriggered" />
        <UiInfoRow label="自动处理错误" :value="store.lastCandidateImportReport.autoErrorCount" />
      </div>
    </UiPanel>

    <UiPanel v-if="store.candidateImportConflicts.length" title="候选人冲突待人工确认">
      <UiTable>
        <thead>
          <tr>
            <UiTh>导入候选人</UiTh>
            <UiTh>疑似匹配</UiTh>
            <UiTh>冲突原因</UiTh>
            <UiTh>操作</UiTh>
          </tr>
        </thead>
        <tbody>
          <tr v-for="conflict in store.candidateImportConflicts" :key="conflict.id">
            <UiTd>
              {{ conflict.imported.name }}
              <br />
              {{ conflict.imported.current_company || "-" }} / {{ conflict.imported.years_of_experience }}年
            </UiTd>
            <UiTd>
              #{{ conflict.existingCandidate.id }} {{ conflict.existingCandidate.name }}
              <br />
              {{ conflict.existingCandidate.current_company || "-" }} / {{ conflict.existingCandidate.years_of_experience }}年
            </UiTd>
            <UiTd>{{ conflict.reasons.join(", ") }}</UiTd>
            <UiTd>
              <div class="flex items-center gap-2.5 mb-2.5 flex-wrap">
                <UiButton variant="ghost" @click="resolveConflict(conflict.id, 'merge')">归并</UiButton>
                <UiButton variant="ghost" @click="resolveConflict(conflict.id, 'create')">新建</UiButton>
                <UiButton variant="ghost" @click="resolveConflict(conflict.id, 'skip')">跳过</UiButton>
              </div>
            </UiTd>
          </tr>
        </tbody>
      </UiTable>
    </UiPanel>

    <UiPanel title="任务列表">
      <UiTable>
        <thead>
          <tr>
            <UiTh>ID</UiTh>
            <UiTh>来源</UiTh>
            <UiTh>模式</UiTh>
            <UiTh>类型</UiTh>
            <UiTh>状态</UiTh>
            <UiTh>重试</UiTh>
            <UiTh>控制</UiTh>
            <UiTh>更新时间</UiTh>
          </tr>
        </thead>
        <tbody>
          <tr v-for="task in store.tasks" :key="task.id">
            <UiTd no-wrap>#{{ task.id }}</UiTd>
            <UiTd>{{ task.source }}</UiTd>
            <UiTd>{{ task.mode }}</UiTd>
            <UiTd>{{ task.task_type }}</UiTd>
            <UiTd>
              <UiBadge :tone="taskStatusTone(task.status)">{{ taskStatusLabel(task.status) }}</UiBadge>
            </UiTd>
            <UiTd>{{ task.retry_count }}</UiTd>
            <UiTd>
              <div class="flex items-center gap-2.5 mb-2.5 flex-wrap">
                <UiButton variant="ghost" :disabled="task.status !== 'RUNNING'" @click="pauseTask(task.id)">暂停</UiButton>
                <UiButton variant="ghost" :disabled="task.status !== 'PAUSED'" @click="resumeTask(task.id)">恢复</UiButton>
                <UiButton variant="ghost" :disabled="task.status === 'SUCCEEDED' || task.status === 'CANCELED'" @click="cancelTask(task.id)">取消</UiButton>
              </div>
            </UiTd>
            <UiTd no-wrap>{{ task.updated_at }}</UiTd>
          </tr>
        </tbody>
      </UiTable>
    </UiPanel>
  </section>
</template>
