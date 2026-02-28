<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import type {
  CrawlMode,
  CrawlTaskPersonRecord,
  CrawlTaskPersonSyncStatus,
  CrawlTaskRecord,
  CrawlTaskScheduleType,
  CrawlTaskSource,
  SortRule,
} from "@doss/shared";
import { useRecruitingStore } from "../stores/recruiting";
import UiBadge from "../components/UiBadge.vue";
import UiButton from "../components/UiButton.vue";
import UiField from "../components/UiField.vue";
import UiInfoRow from "../components/UiInfoRow.vue";
import UiPanel from "../components/UiPanel.vue";
import UiSelect from "../components/UiSelect.vue";
import UiTableFilterPanel from "../components/UiTableFilterPanel.vue";
import UiTable from "../components/UiTable.vue";
import UiTablePagination from "../components/UiTablePagination.vue";
import UiTableToolbar from "../components/UiTableToolbar.vue";
import UiTd from "../components/UiTd.vue";
import UiTh from "../components/UiTh.vue";
import { useToastStore } from "../stores/toast";
import { taskStatusLabel, taskStatusTone } from "../lib/status";
import { includesKeyword } from "../lib/table-filter";
import { clampPage, paginateRows } from "../lib/table-pagination";
import { normalizeSortRules, sortRowsByRules, type SortResolver } from "../lib/table-sort";

const store = useRecruitingStore();
const toast = useToastStore();

const creatingTask = ref(false);
const createModalOpen = ref(false);
const detailDrawerOpen = ref(false);
const peopleModalOpen = ref(false);
const loadingPeople = ref(false);
const syncingPeople = ref(false);
const selectedTaskId = ref<number | null>(null);
const deleteConfirmTask = ref<CrawlTaskRecord | null>(null);
const deletingTaskId = ref<number | null>(null);
const taskAdvancedFilterOpen = ref(false);
const peopleAdvancedFilterOpen = ref(false);
const taskPage = ref(1);
const taskPageSize = ref(10);
const peoplePage = ref(1);
const peoplePageSize = ref(10);

const newTaskForm = reactive({
  source: "all" as CrawlTaskSource,
  mode: "compliant" as CrawlMode,
  localJobId: 0,
  batchSize: 50,
  scheduleType: "ONCE" as CrawlTaskScheduleType,
  scheduleTime: "09:30",
  scheduleDay: 1,
  retryCount: 1,
  retryBackoffMs: 450,
  autoSyncToCandidates: true,
});

const taskTableFilters = reactive({
  quickKeyword: "",
  source: "" as "" | CrawlTaskSource,
  mode: "" as "" | CrawlMode,
  status: "",
  localJobTitle: "",
  autoSync: "" as "" | "yes" | "no",
});

const peopleTableFilters = reactive({
  quickKeyword: "",
  source: "" as "" | CrawlTaskSource,
  syncStatus: "" as "" | CrawlTaskPersonSyncStatus,
  syncedToCandidate: "" as "" | "yes" | "no",
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

const scheduleTypeOptions = [
  { value: "ONCE", label: "单次执行" },
  { value: "DAILY", label: "每日固定时间" },
  { value: "MONTHLY", label: "每月固定日时间" },
];

const autoSyncOptions = [
  { value: true, label: "是" },
  { value: false, label: "否" },
];

const activeJobOptions = computed(() => activeJobs.value.map((job) => ({
  value: job.id,
  label: job.title,
})));

type TaskSortField
  = | "source"
    | "job_title"
    | "mode"
    | "batch_size"
    | "schedule"
    | "auto_sync"
    | "status"
    | "retry_count"
    | "updated_at";
type PeopleSortField
  = | "source"
    | "name"
    | "current_company"
    | "years_of_experience"
    | "sync_status"
    | "candidate_id";

const taskSortOptions: { label: string; value: TaskSortField }[] = [
  { label: "来源", value: "source" },
  { label: "职位", value: "job_title" },
  { label: "模式", value: "mode" },
  { label: "每批人数", value: "batch_size" },
  { label: "调度", value: "schedule" },
  { label: "自动同步", value: "auto_sync" },
  { label: "状态", value: "status" },
  { label: "重试次数", value: "retry_count" },
  { label: "更新时间", value: "updated_at" },
];

const peopleSortOptions: { label: string; value: PeopleSortField }[] = [
  { label: "来源", value: "source" },
  { label: "姓名", value: "name" },
  { label: "当前公司", value: "current_company" },
  { label: "年限", value: "years_of_experience" },
  { label: "同步状态", value: "sync_status" },
  { label: "是否已同步候选人", value: "candidate_id" },
];

const taskSorts = ref<SortRule<TaskSortField>[]>([
  { field: "updated_at", direction: "desc" },
]);
const peopleSorts = ref<SortRule<PeopleSortField>[]>([
  { field: "name", direction: "asc" },
]);

const effectiveTaskSorts = computed(() =>
  normalizeSortRules(
    taskSorts.value,
    taskSortOptions.map((item) => item.value),
  ),
);

const effectivePeopleSorts = computed(() =>
  normalizeSortRules(
    peopleSorts.value,
    peopleSortOptions.map((item) => item.value),
  ),
);

function sortTaskByColumn(payload: { field: string; direction: "asc" | "desc" }) {
  const field = payload.field as TaskSortField;
  if (!taskSortOptions.some((item) => item.value === field)) {
    return;
  }
  const next = [
    { field, direction: payload.direction },
    ...effectiveTaskSorts.value.filter((rule) => rule.field !== field),
  ];
  taskSorts.value = normalizeSortRules(next, taskSortOptions.map((item) => item.value));
}

function sortPeopleByColumn(payload: { field: string; direction: "asc" | "desc" }) {
  const field = payload.field as PeopleSortField;
  if (!peopleSortOptions.some((item) => item.value === field)) {
    return;
  }
  const next = [
    { field, direction: payload.direction },
    ...effectivePeopleSorts.value.filter((rule) => rule.field !== field),
  ];
  peopleSorts.value = normalizeSortRules(next, peopleSortOptions.map((item) => item.value));
}

const taskSourceFilterOptions = [
  { value: "", label: "全部来源" },
  ...sourceOptions.filter((item) => item.value !== "all").map((item) => ({
    value: item.value,
    label: item.label,
  })),
];

const taskModeFilterOptions = [
  { value: "", label: "全部模式" },
  ...modeOptions.map((item) => ({
    value: item.value,
    label: item.label,
  })),
];

const taskStatusFilterOptions = [
  { value: "", label: "全部状态" },
  { value: "PENDING", label: "待执行" },
  { value: "RUNNING", label: "执行中" },
  { value: "PAUSED", label: "暂停" },
  { value: "CANCELED", label: "已取消" },
  { value: "SUCCEEDED", label: "成功" },
  { value: "FAILED", label: "失败" },
];

const yesNoFilterOptions = [
  { value: "", label: "全部" },
  { value: "yes", label: "是" },
  { value: "no", label: "否" },
];

const taskRows = computed(() =>
  store.tasks.filter((task) => {
    const payload = parseTaskPayload(task);
    if (!includesKeyword(
      taskTableFilters.quickKeyword,
      task.source,
      payload.localJobTitle,
      payload.localJobCity,
      task.status,
      task.updated_at,
    )) {
      return false;
    }

    if (taskTableFilters.source && task.source !== taskTableFilters.source) {
      return false;
    }
    if (taskTableFilters.mode && task.mode !== taskTableFilters.mode) {
      return false;
    }
    if (taskTableFilters.status && task.status !== taskTableFilters.status) {
      return false;
    }
    if (!includesKeyword(taskTableFilters.localJobTitle, payload.localJobTitle)) {
      return false;
    }
    if (taskTableFilters.autoSync === "yes" && !payload.autoSyncToCandidates) {
      return false;
    }
    if (taskTableFilters.autoSync === "no" && payload.autoSyncToCandidates) {
      return false;
    }

    return true;
  }),
);

const taskSortResolver: SortResolver<CrawlTaskRecord, TaskSortField> = {
  source: (row) => row.source,
  job_title: (row) => parseTaskPayload(row).localJobTitle,
  mode: (row) => modeLabel(row.mode),
  batch_size: (row) => parseTaskPayload(row).batchSize,
  schedule: (row) => parseTaskPayload(row).scheduleLabel,
  auto_sync: (row) => parseTaskPayload(row).autoSyncToCandidates,
  status: (row) => row.status,
  retry_count: (row) => row.retry_count,
  updated_at: (row) => row.updated_at,
};

const displayTaskRows = computed(() =>
  sortRowsByRules(taskRows.value, effectiveTaskSorts.value, taskSortResolver),
);
const pagedTaskRows = computed(() =>
  paginateRows(displayTaskRows.value, taskPage.value, taskPageSize.value),
);

const peopleSourceFilterOptions = [
  { value: "", label: "全部来源" },
  { value: "boss", label: "Boss" },
  { value: "zhilian", label: "智联" },
  { value: "wuba", label: "58" },
  { value: "lagou", label: "拉勾" },
];

const syncStatusFilterOptions = [
  { value: "", label: "全部同步状态" },
  { value: "UNSYNCED", label: "未同步" },
  { value: "SYNCED", label: "已同步" },
  { value: "FAILED", label: "同步失败" },
];

const peopleRows = computed(() =>
  selectedTaskPeople.value.filter((person) => {
    if (!includesKeyword(
      peopleTableFilters.quickKeyword,
      person.name,
      person.external_id,
      person.current_company,
      person.sync_error_message,
    )) {
      return false;
    }
    if (peopleTableFilters.source && person.source !== peopleTableFilters.source) {
      return false;
    }
    if (peopleTableFilters.syncStatus && person.sync_status !== peopleTableFilters.syncStatus) {
      return false;
    }
    if (peopleTableFilters.syncedToCandidate === "yes" && person.sync_status !== "SYNCED") {
      return false;
    }
    if (peopleTableFilters.syncedToCandidate === "no" && person.sync_status === "SYNCED") {
      return false;
    }
    return true;
  }),
);

const peopleSortResolver: SortResolver<CrawlTaskPersonRecord, PeopleSortField> = {
  source: (row) => row.source,
  name: (row) => row.name,
  current_company: (row) => row.current_company,
  years_of_experience: (row) => row.years_of_experience,
  sync_status: (row) => row.sync_status,
  candidate_id: (row) => row.candidate_id,
};

const displayPeopleRows = computed(() =>
  sortRowsByRules(peopleRows.value, effectivePeopleSorts.value, peopleSortResolver),
);
const pagedPeopleRows = computed(() =>
  paginateRows(displayPeopleRows.value, peoplePage.value, peoplePageSize.value),
);

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

function scheduleLabel(task: CrawlTaskRecord): string {
  const type = (task.schedule_type ?? "ONCE").toUpperCase();
  const scheduleTime = task.schedule_time ?? "09:30";
  if (type === "DAILY") {
    return `每日 ${scheduleTime}`;
  }
  if (type === "MONTHLY") {
    const scheduleDay = task.schedule_day ?? 1;
    return `每月 ${scheduleDay} 日 ${scheduleTime}`;
  }
  return "单次";
}

function parseTaskPayload(task: CrawlTaskRecord) {
  const payload = task.payload ?? {};
  return {
    localJobId: Number(payload.localJobId ?? 0),
    localJobTitle: typeof payload.localJobTitle === "string" ? payload.localJobTitle : "-",
    localJobCity: typeof payload.localJobCity === "string" ? payload.localJobCity : "",
    batchSize: Number(payload.batchSize ?? 0),
    scheduleLabel: scheduleLabel(task),
    retryCount: Number(payload.retryCount ?? 0),
    retryBackoffMs: Number(payload.retryBackoffMs ?? 0),
    autoSyncToCandidates: Boolean(payload.autoSyncToCandidates ?? false),
    nextRunAt: task.next_run_at,
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
  newTaskForm.scheduleType = "ONCE";
  newTaskForm.scheduleTime = "09:30";
  newTaskForm.scheduleDay = 1;
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
      scheduleType: newTaskForm.scheduleType,
      scheduleTime: newTaskForm.scheduleTime,
      scheduleDay: Math.max(1, Math.min(31, Math.trunc(newTaskForm.scheduleDay || 1))),
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

watch(
  () => [
    taskTableFilters.quickKeyword,
    taskTableFilters.source,
    taskTableFilters.mode,
    taskTableFilters.status,
    taskTableFilters.localJobTitle,
    taskTableFilters.autoSync,
    JSON.stringify(effectiveTaskSorts.value),
  ],
  () => {
    taskPage.value = 1;
  },
);

watch(taskPageSize, () => {
  taskPage.value = 1;
});

watch(
  () => displayTaskRows.value.length,
  (total) => {
    taskPage.value = clampPage(taskPage.value, total, taskPageSize.value);
  },
  { immediate: true },
);

watch(
  () => [
    selectedTaskId.value,
    peopleTableFilters.quickKeyword,
    peopleTableFilters.source,
    peopleTableFilters.syncStatus,
    peopleTableFilters.syncedToCandidate,
    JSON.stringify(effectivePeopleSorts.value),
  ],
  () => {
    peoplePage.value = 1;
  },
);

watch(peoplePageSize, () => {
  peoplePage.value = 1;
});

watch(
  () => displayPeopleRows.value.length,
  (total) => {
    peoplePage.value = clampPage(peoplePage.value, total, peoplePageSize.value);
  },
  { immediate: true },
);

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
      <UiTableToolbar
        v-model:quick-keyword="taskTableFilters.quickKeyword"
        v-model:advanced-open="taskAdvancedFilterOpen"
        quick-placeholder="输入来源/职位/状态关键词"
        :show-refresh="false"
        :show-apply="false"
      />

      <UiTableFilterPanel v-model:open="taskAdvancedFilterOpen">
        <div class="grid grid-cols-3 gap-2.5 lt-lg:grid-cols-2 lt-sm:grid-cols-1">
          <UiField label="来源">
            <UiSelect v-model="taskTableFilters.source" :options="taskSourceFilterOptions" />
          </UiField>
          <UiField label="模式">
            <UiSelect v-model="taskTableFilters.mode" :options="taskModeFilterOptions" />
          </UiField>
          <UiField label="状态">
            <UiSelect v-model="taskTableFilters.status" :options="taskStatusFilterOptions" />
          </UiField>
          <UiField label="职位">
            <input v-model="taskTableFilters.localJobTitle" placeholder="职位关键词" />
          </UiField>
          <UiField label="自动同步">
            <UiSelect v-model="taskTableFilters.autoSync" :options="yesNoFilterOptions" />
          </UiField>
        </div>
      </UiTableFilterPanel>

      <UiTable>
        <thead>
          <tr>
            <UiTh sort-field="source" :sorts="effectiveTaskSorts" @sort="sortTaskByColumn">来源</UiTh>
            <UiTh sort-field="job_title" :sorts="effectiveTaskSorts" @sort="sortTaskByColumn">职位</UiTh>
            <UiTh sort-field="mode" :sorts="effectiveTaskSorts" @sort="sortTaskByColumn">模式</UiTh>
            <UiTh sort-field="batch_size" :sorts="effectiveTaskSorts" @sort="sortTaskByColumn">每批人数</UiTh>
            <UiTh sort-field="schedule" :sorts="effectiveTaskSorts" @sort="sortTaskByColumn">调度</UiTh>
            <UiTh>重试参数</UiTh>
            <UiTh sort-field="auto_sync" :sorts="effectiveTaskSorts" @sort="sortTaskByColumn">自动同步</UiTh>
            <UiTh sort-field="status" :sorts="effectiveTaskSorts" @sort="sortTaskByColumn">状态</UiTh>
            <UiTh sort-field="retry_count" :sorts="effectiveTaskSorts" @sort="sortTaskByColumn">重试</UiTh>
            <UiTh sort-field="updated_at" :sorts="effectiveTaskSorts" @sort="sortTaskByColumn">更新时间</UiTh>
            <UiTh>控制</UiTh>
          </tr>
        </thead>
        <tbody>
          <tr v-for="task in pagedTaskRows" :key="task.id">
            <UiTd>{{ task.source }}</UiTd>
            <UiTd>
              {{ parseTaskPayload(task).localJobTitle }}
            </UiTd>
            <UiTd>{{ modeLabel(task.mode) }}</UiTd>
            <UiTd>{{ parseTaskPayload(task).batchSize }}</UiTd>
            <UiTd>{{ parseTaskPayload(task).scheduleLabel }}</UiTd>
            <UiTd>
              {{ parseTaskPayload(task).retryCount }} 次 / {{ parseTaskPayload(task).retryBackoffMs }} ms
            </UiTd>
            <UiTd>{{ parseTaskPayload(task).autoSyncToCandidates ? "是" : "否" }}</UiTd>
            <UiTd>
              <UiBadge :tone="taskStatusTone(task.status)">{{ taskStatusLabel(task.status) }}</UiBadge>
            </UiTd>
            <UiTd>{{ task.retry_count }}</UiTd>
            <UiTd no-wrap>{{ task.updated_at }}</UiTd>
            <UiTd>
              <div class="flex items-center justify-center gap-2 flex-wrap">
                <UiButton variant="ghost" size="sm" @click="openTaskDetail(task.id)">查看</UiButton>
                <UiButton
                  variant="ghost"
                  size="sm"
                  :disabled="toggleTaskDisabled(task)"
                  @click="toggleTask(task)"
                >
                  {{ toggleTaskLabel(task) }}
                </UiButton>
                <UiButton
                  variant="ghost"
                  size="sm"
                  :disabled="deletingTaskId === task.id"
                  @click="askDeleteTask(task)"
                >
                  {{ deletingTaskId === task.id ? "删除中..." : "删除" }}
                </UiButton>
              </div>
            </UiTd>
          </tr>
          <tr v-if="pagedTaskRows.length === 0">
            <UiTd colspan="11" class="text-center text-muted py-6">暂无采集任务</UiTd>
          </tr>
        </tbody>
      </UiTable>
      <UiTablePagination
        v-model:page="taskPage"
        v-model:page-size="taskPageSize"
        :total="displayTaskRows.length"
      />
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
          <UiField label="调度类型">
            <UiSelect v-model="newTaskForm.scheduleType" :options="scheduleTypeOptions" />
          </UiField>
          <UiField label="调度时间">
            <input v-model="newTaskForm.scheduleTime" type="time" />
          </UiField>
          <UiField v-if="newTaskForm.scheduleType === 'MONTHLY'" label="每月日期">
            <input v-model.number="newTaskForm.scheduleDay" type="number" min="1" max="31" step="1" />
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
              <UiBadge :tone="taskStatusTone(selectedTask.status)">{{ taskStatusLabel(selectedTask.status) }}</UiBadge>
            </UiInfoRow>
            <UiInfoRow label="错误码" :value="selectedTask.error_code || '-'" />
            <UiInfoRow label="更新时间" :value="selectedTask.updated_at" />
          </div>
        </UiPanel>

        <UiPanel class="mt-3" title="任务参数">
          <div class="flex flex-col gap-1.5">
            <UiInfoRow label="职位名称" :value="parseTaskPayload(selectedTask).localJobTitle || '-'" />
            <UiInfoRow label="城市" :value="parseTaskPayload(selectedTask).localJobCity || '-'" />
            <UiInfoRow label="每批抓取人数" :value="parseTaskPayload(selectedTask).batchSize" />
            <UiInfoRow label="调度" :value="parseTaskPayload(selectedTask).scheduleLabel" />
            <UiInfoRow label="下次执行时间" :value="parseTaskPayload(selectedTask).nextRunAt || '-'" />
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

        <UiTableToolbar
          v-model:quick-keyword="peopleTableFilters.quickKeyword"
          v-model:advanced-open="peopleAdvancedFilterOpen"
          quick-placeholder="输入姓名/公司/失败原因关键词"
          :show-refresh="false"
          :show-apply="false"
        />

        <UiTableFilterPanel v-model:open="peopleAdvancedFilterOpen">
          <div class="grid grid-cols-3 gap-2.5 lt-lg:grid-cols-2 lt-sm:grid-cols-1">
            <UiField label="来源">
              <UiSelect v-model="peopleTableFilters.source" :options="peopleSourceFilterOptions" />
            </UiField>
            <UiField label="同步状态">
              <UiSelect v-model="peopleTableFilters.syncStatus" :options="syncStatusFilterOptions" />
            </UiField>
            <UiField label="是否已同步候选人">
              <UiSelect v-model="peopleTableFilters.syncedToCandidate" :options="yesNoFilterOptions" />
            </UiField>
          </div>
        </UiTableFilterPanel>

        <UiTable>
          <thead>
            <tr>
              <UiTh sort-field="source" :sorts="effectivePeopleSorts" @sort="sortPeopleByColumn">来源</UiTh>
              <UiTh sort-field="name" :sorts="effectivePeopleSorts" @sort="sortPeopleByColumn">姓名</UiTh>
              <UiTh sort-field="current_company" :sorts="effectivePeopleSorts" @sort="sortPeopleByColumn">当前公司</UiTh>
              <UiTh sort-field="years_of_experience" :sorts="effectivePeopleSorts" @sort="sortPeopleByColumn">年限</UiTh>
              <UiTh sort-field="sync_status" :sorts="effectivePeopleSorts" @sort="sortPeopleByColumn">同步状态</UiTh>
              <UiTh sort-field="candidate_id" :sorts="effectivePeopleSorts" @sort="sortPeopleByColumn">是否已同步到候选人</UiTh>
              <UiTh>失败原因</UiTh>
            </tr>
          </thead>
          <tbody>
            <tr v-for="person in pagedPeopleRows" :key="person.id">
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
            <tr v-if="pagedPeopleRows.length === 0">
              <UiTd colspan="7" class="text-center text-muted py-6">暂无抓取人员</UiTd>
            </tr>
          </tbody>
        </UiTable>
        <UiTablePagination
          v-model:page="peoplePage"
          v-model:page-size="peoplePageSize"
          :total="displayPeopleRows.length"
        />
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
