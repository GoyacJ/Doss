<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import type { CrawlTaskRecord, CrawlTaskSource, CrawlTaskPersonRecord, CrawlTaskPersonSyncStatus, CrawlMode } from "@doss/shared";
import { useRecruitingStore } from "../stores/recruiting";
import UiBadge from "../components/UiBadge.vue";
import UiButton from "../components/UiButton.vue";
import UiField from "../components/UiField.vue";
import UiInfoRow from "../components/UiInfoRow.vue";
import UiPanel from "../components/UiPanel.vue";
import UiSelect from "../components/UiSelect.vue";
import UiTable from "../components/UiTable.vue";
import UiTd from "../components/UiTd.vue";
import UiTh from "../components/UiTh.vue";
import { useToastStore } from "../stores/toast";

const store = useRecruitingStore();
const toast = useToastStore();

type IntervalUnit = "hour" | "minute" | "second";

const creatingTask = ref(false);
const createModalOpen = ref(false);
const detailDrawerOpen = ref(false);
const peopleModalOpen = ref(false);
const loadingPeople = ref(false);
const syncingPeople = ref(false);
const selectedTaskId = ref<number | null>(null);
const deleteConfirmTask = ref<CrawlTaskRecord | null>(null);
const deletingTaskId = ref<number | null>(null);

const newTaskForm = reactive({
  source: "all" as CrawlTaskSource,
  mode: "compliant" as CrawlMode,
  localJobId: 0,
  batchSize: 50,
  crawlIntervalValue: 5,
  crawlIntervalUnit: "minute" as IntervalUnit,
  retryCount: 1,
  retryBackoffMs: 450,
  autoSyncToCandidates: true,
});

const activeJobs = computed(() =>
  store.jobs.filter((item) => item.status !== "STOPPED"),
);

const selectedTask = computed(() =>
  store.tasks.find((item) => item.id === selectedTaskId.value) ?? null,
);

const selectedTaskPeople = computed<CrawlTaskPersonRecord[]>(() => {
  if (!selectedTaskId.value) {
    return [];
  }
  return store.taskPeople[selectedTaskId.value] ?? [];
});

const sourceOptions = [
  { value: "all", label: "全选" },
  { value: "boss", label: "Boss" },
  { value: "zhilian", label: "智联" },
  { value: "wuba", label: "58" },
  { value: "lagou", label: "拉勾" },
];

const modeOptions = [
  { value: "compliant", label: "合规模式" },
  { value: "advanced", label: "高级模式" },
];

const intervalUnitOptions = [
  { value: "hour", label: "小时" },
  { value: "minute", label: "分钟" },
  { value: "second", label: "秒" },
];

const autoSyncOptions = [
  { value: true, label: "是" },
  { value: false, label: "否" },
];

const activeJobOptions = computed(() => activeJobs.value.map((job) => ({
  value: job.id,
  label: job.title,
})));

function resolveErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  if (typeof error === "string" && error.trim()) {
    return error;
  }
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string" && message.trim()) {
      return message;
    }
  }
  return fallback;
}

function toIntervalSeconds(value: number, unit: IntervalUnit): number {
  const safeValue = Math.max(1, Math.trunc(value || 1));
  if (unit === "hour") {
    return safeValue * 3600;
  }
  if (unit === "minute") {
    return safeValue * 60;
  }
  return safeValue;
}

function fromIntervalSeconds(seconds: number): { value: number; unit: IntervalUnit } {
  const normalized = Math.max(1, Math.trunc(seconds || 1));
  if (normalized % 3600 === 0) {
    return {
      value: normalized / 3600,
      unit: "hour",
    };
  }
  if (normalized % 60 === 0) {
    return {
      value: normalized / 60,
      unit: "minute",
    };
  }
  return {
    value: normalized,
    unit: "second",
  };
}

function parseTaskPayload(task: CrawlTaskRecord) {
  const payload = task.payload ?? {};
  const interval = fromIntervalSeconds(Number(payload.crawlIntervalSeconds ?? 300));
  return {
    localJobId: Number(payload.localJobId ?? 0),
    localJobTitle: typeof payload.localJobTitle === "string" ? payload.localJobTitle : "-",
    localJobCity: typeof payload.localJobCity === "string" ? payload.localJobCity : "",
    batchSize: Number(payload.batchSize ?? 0),
    crawlIntervalSeconds: Number(payload.crawlIntervalSeconds ?? 300),
    crawlIntervalLabel: `${interval.value}${interval.unit === "hour" ? "小时" : interval.unit === "minute" ? "分钟" : "秒"}`,
    retryCount: Number(payload.retryCount ?? 0),
    retryBackoffMs: Number(payload.retryBackoffMs ?? 0),
    autoSyncToCandidates: Boolean(payload.autoSyncToCandidates ?? false),
    syncedPeople: Number((task.snapshot ?? {}).syncedPeople ?? 0),
    failedPeople: Number((task.snapshot ?? {}).failedPeople ?? 0),
    fetchedPeople: Number((task.snapshot ?? {}).fetchedPeople ?? 0),
  };
}

function syncStatusLabel(status: CrawlTaskPersonSyncStatus): string {
  if (status === "SYNCED") {
    return "已同步";
  }
  if (status === "FAILED") {
    return "同步失败";
  }
  return "未同步";
}

function syncStatusTone(status: CrawlTaskPersonSyncStatus): "success" | "warning" | "danger" {
  if (status === "SYNCED") {
    return "success";
  }
  if (status === "FAILED") {
    return "danger";
  }
  return "warning";
}

function toggleTaskLabel(task: CrawlTaskRecord): string {
  return task.status === "RUNNING" ? "停止" : "启动";
}

function runStatusLabel(status: CrawlTaskRecord["status"]): string {
  return status === "RUNNING" ? "正在运行" : "待执行";
}

function runStatusTone(status: CrawlTaskRecord["status"]): "success" | "info" {
  return status === "RUNNING" ? "success" : "info";
}

function modeLabel(mode: CrawlMode): string {
  return mode === "advanced" ? "高级模式" : "合规模式";
}

function toggleTaskDisabled(task: CrawlTaskRecord): boolean {
  return task.status === "CANCELED";
}

function resetCreateForm() {
  newTaskForm.source = "all";
  newTaskForm.mode = "compliant";
  newTaskForm.localJobId = activeJobs.value[0]?.id ?? 0;
  newTaskForm.batchSize = 50;
  newTaskForm.crawlIntervalValue = 5;
  newTaskForm.crawlIntervalUnit = "minute";
  newTaskForm.retryCount = 1;
  newTaskForm.retryBackoffMs = 450;
  newTaskForm.autoSyncToCandidates = true;
}

function openCreateModal() {
  resetCreateForm();
  createModalOpen.value = true;
}

function closeCreateModal(force = false) {
  if (creatingTask.value && !force) {
    return;
  }
  createModalOpen.value = false;
}

async function submitCreateTask() {
  if (creatingTask.value) {
    return;
  }
  if (!newTaskForm.localJobId) {
    toast.warning("请选择职位");
    return;
  }

  creatingTask.value = true;
  try {
    await store.createCandidatesTask({
      source: newTaskForm.source,
      mode: newTaskForm.mode,
      localJobId: Number(newTaskForm.localJobId),
      batchSize: Math.max(1, Math.trunc(newTaskForm.batchSize || 1)),
      crawlIntervalSeconds: toIntervalSeconds(newTaskForm.crawlIntervalValue, newTaskForm.crawlIntervalUnit),
      retryCount: Math.max(0, Math.trunc(newTaskForm.retryCount || 0)),
      retryBackoffMs: Math.max(100, Math.trunc(newTaskForm.retryBackoffMs || 100)),
      autoSyncToCandidates: newTaskForm.autoSyncToCandidates,
    });
    closeCreateModal(true);
    toast.success("采集任务已创建，当前状态为待执行");
  } catch (error) {
    toast.danger(error instanceof Error ? error.message : "创建采集任务失败");
  } finally {
    creatingTask.value = false;
  }
}

function openTaskDetail(taskId: number) {
  selectedTaskId.value = taskId;
  detailDrawerOpen.value = true;
}

function closeTaskDetail() {
  detailDrawerOpen.value = false;
}

async function openTaskPeople() {
  if (!selectedTaskId.value) {
    return;
  }
  loadingPeople.value = true;
  try {
    await store.loadTaskPeople(selectedTaskId.value);
    peopleModalOpen.value = true;
  } catch (error) {
    toast.danger(error instanceof Error ? error.message : "加载抓取人员列表失败");
  } finally {
    loadingPeople.value = false;
  }
}

function closeTaskPeople() {
  peopleModalOpen.value = false;
}

async function syncPeople() {
  if (!selectedTaskId.value || syncingPeople.value) {
    return;
  }
  syncingPeople.value = true;
  try {
    const people = await store.syncTaskPeople(selectedTaskId.value);
    const synced = people.filter((item) => item.sync_status === "SYNCED").length;
    const failed = people.filter((item) => item.sync_status === "FAILED").length;
    toast.success(`同步完成：已同步 ${synced}，失败 ${failed}`);
  } catch (error) {
    toast.danger(error instanceof Error ? error.message : "同步失败");
  } finally {
    syncingPeople.value = false;
  }
}

async function toggleTask(task: CrawlTaskRecord) {
  try {
    await store.toggleTaskRunState(task.id);
    toast.success(task.status === "RUNNING" ? "任务已停止" : "任务已恢复并开始执行");
  } catch (error) {
    toast.danger(error instanceof Error ? error.message : "任务状态切换失败");
  }
}

function askDeleteTask(task: CrawlTaskRecord) {
  deleteConfirmTask.value = task;
}

function cancelDeleteTask() {
  if (deletingTaskId.value) {
    return;
  }
  deleteConfirmTask.value = null;
}

async function confirmDeleteTask() {
  const task = deleteConfirmTask.value;
  if (!task) {
    return;
  }

  deletingTaskId.value = task.id;
  try {
    await store.deleteTask(task.id);
    if (selectedTaskId.value === task.id) {
      detailDrawerOpen.value = false;
      peopleModalOpen.value = false;
      selectedTaskId.value = null;
    }
    deleteConfirmTask.value = null;
    toast.success("任务已删除");
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "删除任务失败"));
  } finally {
    deletingTaskId.value = null;
  }
}

onMounted(async () => {
  if (activeJobs.value.length && !newTaskForm.localJobId) {
    newTaskForm.localJobId = activeJobs.value[0]!.id;
  }
  try {
    await store.refreshTasks();
  } catch {
    toast.warning("刷新任务失败");
  }
});
</script>

<template>
  <section class="flex flex-col gap-4">
    <header class="flex items-center justify-between gap-3">
      <h2 class="text-2xl font-700">采集任务</h2>
      <div class="flex items-center gap-2">
        <UiButton variant="secondary" @click="store.refreshTasks">刷新</UiButton>
        <UiButton @click="openCreateModal">新增采集任务</UiButton>
      </div>
    </header>

    <UiPanel title="任务列表">
      <UiTable>
        <thead>
          <tr>
            <UiTh>来源</UiTh>
            <UiTh>职位</UiTh>
            <UiTh>模式</UiTh>
            <UiTh>每批人数</UiTh>
            <UiTh>抓取间隔</UiTh>
            <UiTh>重试参数</UiTh>
            <UiTh>自动同步</UiTh>
            <UiTh>状态</UiTh>
            <UiTh>重试</UiTh>
            <UiTh>更新时间</UiTh>
            <UiTh>控制</UiTh>
          </tr>
        </thead>
        <tbody>
          <tr v-for="task in store.tasks" :key="task.id">
            <UiTd>{{ task.source }}</UiTd>
            <UiTd>
              {{ parseTaskPayload(task).localJobTitle }}
            </UiTd>
            <UiTd>{{ modeLabel(task.mode) }}</UiTd>
            <UiTd>{{ parseTaskPayload(task).batchSize }}</UiTd>
            <UiTd>{{ parseTaskPayload(task).crawlIntervalLabel }}</UiTd>
            <UiTd>
              {{ parseTaskPayload(task).retryCount }} 次 / {{ parseTaskPayload(task).retryBackoffMs }} ms
            </UiTd>
            <UiTd>{{ parseTaskPayload(task).autoSyncToCandidates ? "是" : "否" }}</UiTd>
            <UiTd>
              <UiBadge :tone="runStatusTone(task.status)">{{ runStatusLabel(task.status) }}</UiBadge>
            </UiTd>
            <UiTd>{{ task.retry_count }}</UiTd>
            <UiTd no-wrap>{{ task.updated_at }}</UiTd>
            <UiTd>
              <div class="flex items-center gap-2 flex-wrap">
                <UiButton variant="ghost" @click="openTaskDetail(task.id)">查看</UiButton>
                <UiButton
                  variant="ghost"
                  :disabled="toggleTaskDisabled(task)"
                  @click="toggleTask(task)"
                >
                  {{ toggleTaskLabel(task) }}
                </UiButton>
                <UiButton
                  variant="ghost"
                  :disabled="deletingTaskId === task.id"
                  @click="askDeleteTask(task)"
                >
                  {{ deletingTaskId === task.id ? "删除中..." : "删除" }}
                </UiButton>
              </div>
            </UiTd>
          </tr>
        </tbody>
      </UiTable>
    </UiPanel>
  </section>

  <Teleport to="body">
    <div
      v-if="createModalOpen"
      class="fixed inset-0 z-50 bg-black/42 backdrop-blur-[2px] px-4 py-6 flex items-center justify-center"
      @click.self="closeCreateModal()"
    >
      <UiPanel class="w-full max-w-2xl max-h-[86vh] overflow-y-auto">
        <template #header>
          <div class="flex items-center justify-between gap-2 mb-2.5">
            <h3 class="text-lg font-700">新增采集任务</h3>
            <UiButton variant="ghost" @click="closeCreateModal()">关闭</UiButton>
          </div>
        </template>

        <div class="grid grid-cols-2 gap-2.5 mb-2.5 lt-lg:grid-cols-1">
          <UiField label="来源平台">
            <UiSelect v-model="newTaskForm.source" :options="sourceOptions" />
          </UiField>
          <UiField label="采集模式">
            <UiSelect v-model="newTaskForm.mode" :options="modeOptions" />
          </UiField>
          <UiField label="职位">
            <UiSelect
              v-model="newTaskForm.localJobId"
              :options="activeJobOptions"
              value-type="number"
              placeholder="请选择职位"
            />
          </UiField>
          <UiField label="每批抓取人数">
            <input v-model.number="newTaskForm.batchSize" type="number" min="1" max="500" step="1" />
          </UiField>
          <UiField label="抓取间隔">
            <div class="grid grid-cols-[1fr_110px] gap-2">
              <input v-model.number="newTaskForm.crawlIntervalValue" type="number" min="1" step="1" />
              <UiSelect v-model="newTaskForm.crawlIntervalUnit" :options="intervalUnitOptions" />
            </div>
          </UiField>
          <UiField label="失败重试次数">
            <input v-model.number="newTaskForm.retryCount" type="number" min="0" max="8" step="1" />
          </UiField>
          <UiField label="重试退避(ms)">
            <input v-model.number="newTaskForm.retryBackoffMs" type="number" min="100" max="10000" step="50" />
          </UiField>
          <UiField label="自动同步到候选人">
            <UiSelect v-model="newTaskForm.autoSyncToCandidates" :options="autoSyncOptions" value-type="boolean" />
          </UiField>
        </div>

        <div class="flex items-center justify-end gap-2">
          <UiButton variant="secondary" @click="closeCreateModal()">取消</UiButton>
          <UiButton :disabled="creatingTask" @click="submitCreateTask">
            {{ creatingTask ? "创建中..." : "创建任务" }}
          </UiButton>
        </div>
      </UiPanel>
    </div>
  </Teleport>

  <Teleport to="body">
    <div
      v-if="detailDrawerOpen && selectedTask"
      class="fixed inset-0 z-50 pointer-events-none"
    >
      <div class="absolute inset-0 bg-black/26 pointer-events-auto" @click="closeTaskDetail" />
      <aside class="absolute right-0 top-0 h-full w-full max-w-xl bg-bg border-l border-line p-4 overflow-y-auto pointer-events-auto">
        <div class="flex items-center justify-between gap-2 mb-3">
          <h3 class="text-lg font-700">任务详情</h3>
          <UiButton variant="ghost" @click="closeTaskDetail">关闭</UiButton>
        </div>

        <UiPanel title="基础信息">
          <div class="flex flex-col gap-1.5">
            <UiInfoRow label="来源" :value="selectedTask.source" />
            <UiInfoRow label="模式" :value="modeLabel(selectedTask.mode)" />
            <UiInfoRow label="状态">
              <UiBadge :tone="runStatusTone(selectedTask.status)">{{ runStatusLabel(selectedTask.status) }}</UiBadge>
            </UiInfoRow>
            <UiInfoRow label="更新时间" :value="selectedTask.updated_at" />
          </div>
        </UiPanel>

        <UiPanel class="mt-3" title="任务参数">
          <div class="flex flex-col gap-1.5">
            <UiInfoRow label="职位名称" :value="parseTaskPayload(selectedTask).localJobTitle || '-'" />
            <UiInfoRow label="城市" :value="parseTaskPayload(selectedTask).localJobCity || '-'" />
            <UiInfoRow label="每批抓取人数" :value="parseTaskPayload(selectedTask).batchSize" />
            <UiInfoRow label="抓取间隔" :value="parseTaskPayload(selectedTask).crawlIntervalLabel" />
            <UiInfoRow label="失败重试次数" :value="parseTaskPayload(selectedTask).retryCount" />
            <UiInfoRow label="重试退避(ms)" :value="parseTaskPayload(selectedTask).retryBackoffMs" />
            <UiInfoRow label="自动同步到候选人" :value="parseTaskPayload(selectedTask).autoSyncToCandidates ? '是' : '否'" />
          </div>
        </UiPanel>

        <UiPanel class="mt-3" title="最近执行摘要">
          <div class="flex flex-col gap-1.5">
            <UiInfoRow label="本轮抓取" :value="parseTaskPayload(selectedTask).fetchedPeople" />
            <UiInfoRow label="已同步" :value="parseTaskPayload(selectedTask).syncedPeople" />
            <UiInfoRow label="同步失败" :value="parseTaskPayload(selectedTask).failedPeople" />
          </div>
          <div class="mt-3">
            <UiButton :disabled="loadingPeople" @click="openTaskPeople">
              {{ loadingPeople ? "加载中..." : "查看抓取人员列表" }}
            </UiButton>
          </div>
        </UiPanel>
      </aside>
    </div>
  </Teleport>

  <Teleport to="body">
    <div
      v-if="peopleModalOpen && selectedTask"
      class="fixed inset-0 z-[60] bg-black/42 backdrop-blur-[2px] px-4 py-6 flex items-center justify-center"
      @click.self="closeTaskPeople"
    >
      <UiPanel class="w-full max-w-5xl max-h-[88vh] overflow-y-auto">
        <template #header>
          <div class="flex items-center justify-between gap-2 mb-2.5">
            <h3 class="text-lg font-700">抓取人员列表</h3>
            <div class="flex items-center gap-2">
              <UiButton variant="secondary" :disabled="loadingPeople" @click="openTaskPeople">刷新</UiButton>
              <UiButton :disabled="syncingPeople" @click="syncPeople">
                {{ syncingPeople ? "同步中..." : "同步" }}
              </UiButton>
              <UiButton variant="ghost" @click="closeTaskPeople">关闭</UiButton>
            </div>
          </div>
        </template>

        <UiTable>
          <thead>
            <tr>
              <UiTh>来源</UiTh>
              <UiTh>姓名</UiTh>
              <UiTh>当前公司</UiTh>
              <UiTh>年限</UiTh>
              <UiTh>同步状态</UiTh>
              <UiTh>是否已同步到候选人</UiTh>
              <UiTh>失败原因</UiTh>
            </tr>
          </thead>
          <tbody>
            <tr v-for="person in selectedTaskPeople" :key="person.id">
              <UiTd>{{ person.source }}</UiTd>
              <UiTd>
                <div class="flex flex-col gap-0.5">
                  <span>{{ person.name }}</span>
                  <span class="text-muted text-[0.8rem]">{{ person.external_id || '-' }}</span>
                </div>
              </UiTd>
              <UiTd>{{ person.current_company || '-' }}</UiTd>
              <UiTd>{{ person.years_of_experience }}</UiTd>
              <UiTd>
                <UiBadge :tone="syncStatusTone(person.sync_status)">{{ syncStatusLabel(person.sync_status) }}</UiBadge>
              </UiTd>
              <UiTd>{{ person.sync_status === "SYNCED" ? "是" : "否" }}</UiTd>
              <UiTd>{{ person.sync_error_message || '-' }}</UiTd>
            </tr>
            <tr v-if="selectedTaskPeople.length === 0">
              <UiTd colspan="7" class="text-center text-muted py-6">暂无抓取人员</UiTd>
            </tr>
          </tbody>
        </UiTable>
      </UiPanel>
    </div>
  </Teleport>

  <Teleport to="body">
    <div
      v-if="deleteConfirmTask"
      class="fixed inset-0 z-[85] flex items-center justify-center bg-black/35 p-4"
      @click.self="cancelDeleteTask"
    >
      <div class="w-full max-w-md">
        <UiPanel title="删除采集任务">
          <p class="m-0">
            确认删除任务「{{ parseTaskPayload(deleteConfirmTask).localJobTitle }}」吗？此操作不可撤销。
          </p>
          <div class="mt-4 flex flex-wrap justify-end gap-2">
            <UiButton
              variant="ghost"
              :disabled="deletingTaskId === deleteConfirmTask.id"
              @click="cancelDeleteTask"
            >
              取消
            </UiButton>
            <UiButton
              :disabled="deletingTaskId === deleteConfirmTask.id"
              @click="confirmDeleteTask"
            >
              {{ deletingTaskId === deleteConfirmTask.id ? "删除中..." : "确认删除" }}
            </UiButton>
          </div>
        </UiPanel>
      </div>
    </div>
  </Teleport>
</template>
