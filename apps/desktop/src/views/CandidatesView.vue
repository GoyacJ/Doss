<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import type { CandidateGender, CandidateRecord, PipelineStage, SortRule } from "@doss/shared";
import { useRoute, useRouter } from "vue-router";
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
import { buildCandidateManualPayload } from "../lib/candidate-form";
import { formatStageLabel } from "../lib/pipeline";
import { stageTone } from "../lib/status";
import { normalizeSortRules } from "../lib/table-sort";
import { listCandidatesPage } from "../services/backend";
import { useRecruitingStore } from "../stores/recruiting";
import { useToastStore } from "../stores/toast";

const store = useRecruitingStore();
const toast = useToastStore();
const router = useRouter();
const route = useRoute();

const loading = ref(false);
const page = ref(1);
const pageSize = ref(10);
const total = ref(0);
const rows = ref<CandidateRecord[]>([]);

const filters = reactive({
  jobId: 0,
  nameLike: "",
  stage: "" as PipelineStage | "",
});
const advancedFilterOpen = ref(false);

type CandidateSortField
  = | "name"
    | "current_company"
    | "job_title"
    | "score"
    | "stage"
    | "years_of_experience"
    | "updated_at"
    | "created_at";

const sortOptions: { label: string; value: CandidateSortField }[] = [
  { label: "姓名", value: "name" },
  { label: "当前公司", value: "current_company" },
  { label: "职位", value: "job_title" },
  { label: "评分", value: "score" },
  { label: "阶段", value: "stage" },
  { label: "工作年限", value: "years_of_experience" },
  { label: "更新时间", value: "updated_at" },
  { label: "创建时间", value: "created_at" },
];

const sorts = ref<SortRule<CandidateSortField>[]>([
  { field: "job_title", direction: "asc" },
  { field: "score", direction: "desc" },
  { field: "updated_at", direction: "desc" },
]);
const effectiveSorts = computed(() =>
  normalizeSortRules(
    sorts.value,
    sortOptions.map((item) => item.value),
  ),
);

function sortByColumn(payload: { field: string; direction: "asc" | "desc" }) {
  const field = payload.field as CandidateSortField;
  if (!sortOptions.some((item) => item.value === field)) {
    return;
  }
  const next = [
    { field, direction: payload.direction },
    ...effectiveSorts.value.filter((rule) => rule.field !== field),
  ];
  sorts.value = normalizeSortRules(next, sortOptions.map((item) => item.value));
}

const drawerOpen = ref(false);
const drawerLoading = ref(false);
const selectedCandidateId = ref<number | null>(null);
const actionLoading = ref(false);
const deletingCandidateId = ref<number | null>(null);
const deleteConfirmCandidate = ref<CandidateRecord | null>(null);
const createModalOpen = ref(false);
const creatingCandidate = ref(false);
const createResumeFile = ref<File | null>(null);
const createResumeInput = ref<HTMLInputElement | null>(null);

const createForm = reactive({
  name: "",
  currentCompany: "",
  jobId: 0,
  yearsOfExperience: "0",
  score: "",
  age: "",
  gender: "" as CandidateGender | "",
  address: "",
  phone: "",
  email: "",
  tagsText: "",
  enableOcr: false,
});

const selectedCandidate = computed(() => {
  if (!selectedCandidateId.value) {
    return null;
  }
  return rows.value.find((item) => item.id === selectedCandidateId.value)
    ?? store.candidates.find((item) => item.id === selectedCandidateId.value)
    ?? null;
});

const selectedScreening = computed(() => {
  if (!selectedCandidateId.value) {
    return [];
  }
  return store.screeningResults[selectedCandidateId.value] ?? [];
});

const selectedEvents = computed(() => {
  if (!selectedCandidateId.value) {
    return [];
  }
  return store.pipelineEvents[selectedCandidateId.value] ?? [];
});

const selectedStructuredMeta = computed(() => {
  const structured = asRecord(selectedScreening.value[0]?.structured_result);
  const weights = asRecord(structured?.weights);
  const t0 = asRecord(weights?.t0);
  const t1 = asRecord(weights?.t1);
  const t2 = asRecord(weights?.t2);
  return {
    overallComment: asString(structured?.overall_comment) || "-",
    riskAlerts: asStringList(structured?.risk_alerts),
    t0Rule: asString(t0?.rule) || "-",
    t1Template: asString(t1?.template) || "-",
    t2Bonus: asNumber(t2?.bonus),
  };
});

const stageOptions = [
  { value: "", label: "全部阶段" },
  { value: "NEW", label: "新候选" },
  { value: "SCREENING", label: "初筛中" },
  { value: "INTERVIEW", label: "面试中" },
  { value: "HOLD", label: "搁置" },
  { value: "REJECTED", label: "已淘汰" },
  { value: "OFFERED", label: "已录用" },
];

const jobOptions = computed(() => [
  { value: 0, label: "全部职位" },
  ...store.jobs.map((job) => ({ value: job.id, label: `${job.title} · ${job.company}` })),
]);

const createJobOptions = computed(() => [
  { value: 0, label: "不绑定职位" },
  ...store.jobs.map((job) => ({ value: job.id, label: `${job.title} · ${job.company}` })),
]);

const genderOptions = [
  { value: "", label: "未设置" },
  { value: "male", label: "男" },
  { value: "female", label: "女" },
  { value: "other", label: "其他" },
];

const resumeAccept = ".pdf,.docx,.txt,.md,.png,.jpg,.jpeg,.bmp,.tif,.tiff";
const selectedResumeFileName = computed(() => createResumeFile.value?.name || "");

function asRecord(value: unknown): Record<string, unknown> | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

function asString(value: unknown): string | null {
  if (typeof value === "string" && value.trim()) {
    return value.trim();
  }
  return null;
}

function asNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  return null;
}

function asStringList(value: unknown): string[] {
  if (!Array.isArray(value)) {
    return [];
  }
  return value
    .filter((item): item is string => typeof item === "string")
    .map((item) => item.trim())
    .filter(Boolean);
}

function resolveErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  if (typeof error === "string" && error.trim()) {
    return error;
  }
  return fallback;
}

async function loadRows() {
  loading.value = true;
  try {
    const data = await listCandidatesPage({
      page: page.value,
      page_size: pageSize.value,
      job_id: filters.jobId > 0 ? filters.jobId : undefined,
      name_like: filters.nameLike.trim() || undefined,
      stage: filters.stage || undefined,
      sorts: effectiveSorts.value,
    });
    rows.value = data.items;
    total.value = data.total;
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "候选人列表加载失败"));
  } finally {
    loading.value = false;
  }
}

function applyFilters() {
  page.value = 1;
  void loadRows();
}

function resetCreateForm() {
  createForm.name = "";
  createForm.currentCompany = "";
  createForm.jobId = 0;
  createForm.yearsOfExperience = "0";
  createForm.score = "";
  createForm.age = "";
  createForm.gender = "";
  createForm.address = "";
  createForm.phone = "";
  createForm.email = "";
  createForm.tagsText = "";
  createForm.enableOcr = false;
  createResumeFile.value = null;
  if (createResumeInput.value) {
    createResumeInput.value.value = "";
  }
}

function openCreateCandidateModal() {
  resetCreateForm();
  createModalOpen.value = true;
}

function closeCreateCandidateModal(force = false) {
  if (creatingCandidate.value && !force) {
    return;
  }
  createModalOpen.value = false;
}

function onCreateResumeChange(event: Event) {
  const target = event.target as HTMLInputElement | null;
  const file = target?.files?.[0];
  createResumeFile.value = file ?? null;
}

function clearCreateResume() {
  createResumeFile.value = null;
  if (createResumeInput.value) {
    createResumeInput.value.value = "";
  }
}

async function saveCandidate() {
  if (creatingCandidate.value) {
    return;
  }

  const built = buildCandidateManualPayload({
    name: createForm.name,
    currentCompany: createForm.currentCompany,
    jobId: createForm.jobId,
    yearsOfExperience: createForm.yearsOfExperience,
    score: createForm.score,
    age: createForm.age,
    gender: createForm.gender,
    address: createForm.address,
    phone: createForm.phone,
    email: createForm.email,
    tagsText: createForm.tagsText,
  });
  if (!built.ok) {
    toast.warning(built.error);
    return;
  }

  creatingCandidate.value = true;
  const resumeFile = createResumeFile.value;
  let resumeErrorMessage: string | null = null;

  try {
    const created = await store.addCandidate(built.payload);

    if (resumeFile) {
      try {
        await store.importResumeFileAndAnalyze({
          candidateId: created.id,
          file: resumeFile,
          enableOcr: createForm.enableOcr,
          jobId: created.job_id ?? undefined,
        });
      } catch (error) {
        resumeErrorMessage = resolveErrorMessage(error, "简历上传失败");
      }
    }

    await loadRows();
    closeCreateCandidateModal(true);
    resetCreateForm();

    if (resumeErrorMessage) {
      toast.warning(`候选人已创建，但简历处理失败：${resumeErrorMessage}`);
      return;
    }

    toast.success(resumeFile ? "候选人和简历已保存" : "候选人已创建");
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "创建候选人失败"));
  } finally {
    creatingCandidate.value = false;
  }
}

async function openDrawer(candidate: CandidateRecord) {
  selectedCandidateId.value = candidate.id;
  drawerOpen.value = true;
  drawerLoading.value = true;
  try {
    await store.loadCandidateContext(candidate.id);
  } catch (error) {
    toast.warning(resolveErrorMessage(error, "候选人上下文加载失败"));
  } finally {
    drawerLoading.value = false;
  }
}

function askRemoveCandidate(candidate: CandidateRecord) {
  if (deletingCandidateId.value) {
    return;
  }
  deleteConfirmCandidate.value = candidate;
}

function cancelRemoveCandidate() {
  if (deletingCandidateId.value) {
    return;
  }
  deleteConfirmCandidate.value = null;
}

async function removeCandidate() {
  const candidate = deleteConfirmCandidate.value;
  if (!candidate) {
    return;
  }

  deletingCandidateId.value = candidate.id;
  try {
    await store.deleteCandidate(candidate.id);
    deleteConfirmCandidate.value = null;
    if (selectedCandidateId.value === candidate.id) {
      selectedCandidateId.value = null;
      drawerOpen.value = false;
    }

    if (rows.value.length <= 1 && page.value > 1) {
      page.value -= 1;
    } else {
      await loadRows();
    }
    toast.success("候选人已删除");
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "删除候选人失败"));
  } finally {
    deletingCandidateId.value = null;
  }
}

async function rerunScreening() {
  if (!selectedCandidate.value || actionLoading.value) {
    return;
  }
  actionLoading.value = true;
  try {
    await store.runScreening(selectedCandidate.value.id, selectedCandidate.value.job_id ?? undefined);
    await store.loadCandidateContext(selectedCandidate.value.id);
    toast.success("初筛结果已更新");
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "重新分析失败"));
  } finally {
    actionLoading.value = false;
  }
}

function goInterview() {
  if (!selectedCandidate.value) {
    return;
  }
  router.push({
    path: "/interview",
    query: {
      candidateId: String(selectedCandidate.value.id),
    },
  });
}

async function rejectCandidate() {
  if (!selectedCandidate.value || actionLoading.value) {
    return;
  }
  actionLoading.value = true;
  try {
    await store.finalizeHiringDecision({
      candidate_id: selectedCandidate.value.id,
      job_id: selectedCandidate.value.job_id ?? undefined,
      final_decision: "NO_HIRE",
      reason_code: "interview_reject",
    });
    await Promise.all([loadRows(), store.loadCandidateContext(selectedCandidate.value.id)]);
    toast.success("已标记遗憾");
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "遗憾操作失败"));
  } finally {
    actionLoading.value = false;
  }
}

function screeningTone(recommendation: "PASS" | "REVIEW" | "REJECT") {
  if (recommendation === "PASS") {
    return "success";
  }
  if (recommendation === "REVIEW") {
    return "warning";
  }
  return "danger";
}

watch(page, () => {
  void loadRows();
});

watch(pageSize, () => {
  if (page.value !== 1) {
    page.value = 1;
    return;
  }
  void loadRows();
});

watch(
  sorts,
  () => {
    page.value = 1;
    void loadRows();
  },
  { deep: true },
);

onMounted(async () => {
  await Promise.allSettled([
    store.bootstrap(),
    loadRows(),
  ]);

  const candidateId = Number(route.query.candidateId);
  if (Number.isFinite(candidateId) && candidateId > 0) {
    const candidate = rows.value.find((item) => item.id === candidateId);
    if (candidate) {
      await openDrawer(candidate);
    }
  }
});
</script>

<template>
  <section class="flex flex-col gap-4">
    <header class="flex items-center justify-between gap-3">
      <h2 class="text-2xl font-700">候选人池</h2>
      <UiButton @click="openCreateCandidateModal">创建候选人</UiButton>
    </header>

    <UiPanel title="候选人列表">
      <UiTableToolbar
        v-model:quick-keyword="filters.nameLike"
        v-model:advanced-open="advancedFilterOpen"
        :disabled="loading"
        quick-placeholder="输入姓名关键词"
        @apply="applyFilters"
        @refresh="loadRows"
      />

      <UiTableFilterPanel v-model:open="advancedFilterOpen">
        <div class="grid grid-cols-2 gap-2.5 lt-sm:grid-cols-1">
          <UiField label="职位筛选">
            <UiSelect v-model="filters.jobId" :options="jobOptions" value-type="number" />
          </UiField>
          <UiField label="阶段筛选">
            <UiSelect v-model="filters.stage" :options="stageOptions" />
          </UiField>
        </div>
      </UiTableFilterPanel>

      <UiTable>
        <thead>
          <tr>
            <UiTh sort-field="name" :sorts="effectiveSorts" @sort="sortByColumn">姓名</UiTh>
            <UiTh sort-field="current_company" :sorts="effectiveSorts" @sort="sortByColumn">当前公司</UiTh>
            <UiTh sort-field="job_title" :sorts="effectiveSorts" @sort="sortByColumn">职位</UiTh>
            <UiTh sort-field="score" :sorts="effectiveSorts" @sort="sortByColumn">评分</UiTh>
            <UiTh sort-field="stage" :sorts="effectiveSorts" @sort="sortByColumn">阶段</UiTh>
            <UiTh>操作</UiTh>
          </tr>
        </thead>
        <tbody>
          <tr v-for="candidate in rows" :key="candidate.id">
            <UiTd>#{{ candidate.id }} {{ candidate.name }}</UiTd>
            <UiTd>{{ candidate.current_company || "-" }}</UiTd>
            <UiTd>{{ candidate.job_title || (candidate.job_id ? `职位 #${candidate.job_id}` : "-") }}</UiTd>
            <UiTd>{{ candidate.score ?? "-" }}</UiTd>
            <UiTd>
              <UiBadge :tone="stageTone(candidate.stage)">{{ formatStageLabel(candidate.stage) }}</UiBadge>
            </UiTd>
            <UiTd>
              <div class="flex items-center justify-center gap-1 flex-wrap">
                <UiButton variant="ghost" :disabled="deletingCandidateId === candidate.id" @click="openDrawer(candidate)">查看详情</UiButton>
                <UiButton variant="ghost" :disabled="Boolean(deletingCandidateId)" @click="askRemoveCandidate(candidate)">
                  {{ deletingCandidateId === candidate.id ? "删除中..." : "删除" }}
                </UiButton>
              </div>
            </UiTd>
          </tr>
          <tr v-if="!loading && rows.length === 0">
            <UiTd colspan="6" class="text-center text-muted py-6">暂无数据</UiTd>
          </tr>
        </tbody>
      </UiTable>

      <UiTablePagination
        v-model:page="page"
        v-model:page-size="pageSize"
        :total="total"
        :disabled="loading"
      />
    </UiPanel>
  </section>

  <Teleport to="body">
    <div
      v-if="createModalOpen"
      class="fixed inset-0 z-[78] flex items-center justify-center bg-black/35 p-4"
      @click.self="closeCreateCandidateModal()"
    >
      <div class="w-full max-w-4xl">
        <UiPanel title="创建候选人">
          <div class="grid grid-cols-2 gap-2.5 lt-lg:grid-cols-1">
            <UiField label="姓名">
              <input v-model="createForm.name" placeholder="例如：张三" />
            </UiField>
            <UiField label="绑定职位">
              <UiSelect v-model="createForm.jobId" :options="createJobOptions" value-type="number" />
            </UiField>
            <UiField label="当前公司">
              <input v-model="createForm.currentCompany" placeholder="当前任职公司" />
            </UiField>
            <UiField label="工作年限（年）">
              <input v-model="createForm.yearsOfExperience" type="number" min="0" step="0.5" placeholder="例如：5" />
            </UiField>
            <UiField label="评分（0-100）">
              <input v-model="createForm.score" type="number" min="0" max="100" step="0.1" placeholder="可选" />
            </UiField>
            <UiField label="年龄">
              <input v-model="createForm.age" type="number" min="0" step="1" placeholder="可选" />
            </UiField>
            <UiField label="性别">
              <UiSelect v-model="createForm.gender" :options="genderOptions" />
            </UiField>
            <UiField label="电话">
              <input v-model="createForm.phone" placeholder="可选" />
            </UiField>
            <UiField label="邮箱">
              <input v-model="createForm.email" placeholder="可选" />
            </UiField>
            <UiField label="地址">
              <input v-model="createForm.address" placeholder="可选" />
            </UiField>
          </div>

          <UiField class="mt-2.5" label="标签" help="多个标签可用英文逗号、中文逗号或换行分隔">
            <input v-model="createForm.tagsText" placeholder="例如：Vue, TypeScript, 稳定" />
          </UiField>

          <UiField class="mt-2.5" label="简历上传" help="支持 .pdf .docx .txt .md 以及图片格式">
            <input
              ref="createResumeInput"
              type="file"
              :accept="resumeAccept"
              :disabled="creatingCandidate"
              @change="onCreateResumeChange"
            />
            <div v-if="selectedResumeFileName" class="mt-2 flex items-center justify-between gap-2">
              <span class="text-sm text-muted truncate">{{ selectedResumeFileName }}</span>
              <UiButton variant="ghost" :disabled="creatingCandidate" @click="clearCreateResume">移除</UiButton>
            </div>
          </UiField>

          <UiField class="mt-2.5" label="简历解析选项">
            <label class="inline-flex items-center gap-2 text-sm text-muted">
              <input v-model="createForm.enableOcr" type="checkbox" :disabled="creatingCandidate" />
              <span>文本为空时启用 OCR（适用于扫描件）</span>
            </label>
          </UiField>

          <div class="mt-4 flex flex-wrap justify-end gap-2">
            <UiButton variant="ghost" :disabled="creatingCandidate" @click="closeCreateCandidateModal()">取消</UiButton>
            <UiButton :disabled="creatingCandidate" @click="saveCandidate">
              {{ creatingCandidate ? "保存中..." : "创建候选人" }}
            </UiButton>
          </div>
        </UiPanel>
      </div>
    </div>
  </Teleport>

  <Teleport to="body">
    <div
      v-if="drawerOpen && selectedCandidate"
      class="fixed inset-0 z-50 pointer-events-none"
    >
      <div class="absolute inset-0 bg-black/26 pointer-events-auto" @click="drawerOpen = false" />
      <aside class="absolute right-0 top-0 h-full w-full max-w-2xl bg-bg border-l border-line p-4 overflow-y-auto pointer-events-auto">
        <div class="flex items-center justify-between gap-2 mb-3">
          <h3 class="text-lg font-700">候选人详情</h3>
          <UiButton variant="ghost" @click="drawerOpen = false">关闭</UiButton>
        </div>

        <UiPanel>
          <template #header>
            <div class="flex items-center justify-between gap-2 mb-2.5">
              <h4 class="m-0 text-base font-700">{{ selectedCandidate.name }}</h4>
              <UiBadge :tone="stageTone(selectedCandidate.stage)">{{ formatStageLabel(selectedCandidate.stage) }}</UiBadge>
            </div>
          </template>

          <div class="grid grid-cols-2 gap-2.5 lt-lg:grid-cols-1">
            <UiInfoRow label="当前公司" :value="selectedCandidate.current_company || '-'" />
            <UiInfoRow label="职位" :value="selectedCandidate.job_title || (selectedCandidate.job_id ? `职位 #${selectedCandidate.job_id}` : '-')" />
            <UiInfoRow label="评分" :value="selectedCandidate.score ?? '-'" />
            <UiInfoRow label="年龄" :value="selectedCandidate.age ?? '-'" />
            <UiInfoRow label="电话" :value="selectedCandidate.phone_masked || '-'" />
            <UiInfoRow label="邮箱" :value="selectedCandidate.email_masked || '-'" />
            <UiInfoRow label="候选人简历" :value="selectedStructuredMeta.overallComment" />
          </div>

          <div class="mt-3 flex flex-wrap gap-2">
            <UiButton :disabled="actionLoading" @click="rerunScreening">重新分析</UiButton>
            <UiButton variant="secondary" :disabled="actionLoading" @click="goInterview">邀约面试</UiButton>
            <UiButton variant="ghost" :disabled="actionLoading" @click="rejectCandidate">遗憾</UiButton>
          </div>
        </UiPanel>

        <UiPanel v-if="selectedScreening.length" class="mt-3" title="结构化 AI 评估">
          <div class="flex items-center gap-2 mb-2 flex-wrap">
            <UiBadge :tone="screeningTone(selectedScreening[0].recommendation)">{{ selectedScreening[0].recommendation }}</UiBadge>
            <UiBadge :tone="selectedScreening[0].risk_level === 'HIGH' ? 'danger' : selectedScreening[0].risk_level === 'MEDIUM' ? 'warning' : 'info'">
              风险 {{ selectedScreening[0].risk_level }}
            </UiBadge>
          </div>

          <div class="grid grid-cols-2 gap-2 lt-lg:grid-cols-1">
            <UiInfoRow label="T0" :value="selectedScreening[0].t0_score" />
            <UiInfoRow label="T1" :value="selectedScreening[0].t1_score" />
            <UiInfoRow label="精筛" :value="selectedScreening[0].fine_score" />
            <UiInfoRow label="总分" :value="selectedScreening[0].overall_score" />
            <UiInfoRow label="T0 规则" :value="selectedStructuredMeta.t0Rule" />
            <UiInfoRow label="T1 模板" :value="selectedStructuredMeta.t1Template" />
            <UiInfoRow label="T2 加分" :value="selectedStructuredMeta.t2Bonus ?? '-'" />
            <UiInfoRow label="总评" :value="selectedStructuredMeta.overallComment" />
          </div>

          <div v-if="selectedStructuredMeta.riskAlerts.length" class="mt-2">
            <p class="m-0 mb-1 font-600">风险提示</p>
            <ul class="mt-1 pl-4.5">
              <li v-for="item in selectedStructuredMeta.riskAlerts" :key="item">{{ item }}</li>
            </ul>
          </div>
        </UiPanel>

        <UiPanel v-if="selectedEvents.length" class="mt-3" title="流转历史">
          <ul class="mt-2 flex flex-col gap-2">
            <li
              v-for="item in selectedEvents"
              :key="item.id"
              class="border border-line rounded-xl px-2.5 py-2 flex items-center justify-between gap-2 flex-wrap"
            >
              <div class="flex items-center gap-1.5">
                <UiBadge :tone="stageTone(item.from_stage)">{{ formatStageLabel(item.from_stage) }}</UiBadge>
                <span class="text-muted">→</span>
                <UiBadge :tone="stageTone(item.to_stage)">{{ formatStageLabel(item.to_stage) }}</UiBadge>
              </div>
              <span class="text-[0.82rem] text-muted">{{ item.created_at }}</span>
              <p v-if="item.note" class="m-0 w-full text-[0.82rem] text-muted">备注: {{ item.note }}</p>
            </li>
          </ul>
        </UiPanel>

        <p v-if="drawerLoading" class="m-0 mt-3 text-sm text-muted">正在加载候选人上下文...</p>
      </aside>
    </div>
  </Teleport>

  <Teleport to="body">
    <div
      v-if="deleteConfirmCandidate"
      class="fixed inset-0 z-[85] flex items-center justify-center bg-black/35 p-4"
      @click.self="cancelRemoveCandidate()"
    >
      <div class="w-full max-w-md">
        <UiPanel title="删除候选人">
          <p class="m-0">
            确认删除候选人「{{ deleteConfirmCandidate.name }}」吗？此操作不可撤销。
          </p>
          <div class="mt-4 flex flex-wrap justify-end gap-2">
            <UiButton
              variant="ghost"
              :disabled="deletingCandidateId === deleteConfirmCandidate.id"
              @click="cancelRemoveCandidate()"
            >
              取消
            </UiButton>
            <UiButton
              :disabled="deletingCandidateId === deleteConfirmCandidate.id"
              @click="removeCandidate()"
            >
              {{ deletingCandidateId === deleteConfirmCandidate.id ? "删除中..." : "确认删除" }}
            </UiButton>
          </div>
        </UiPanel>
      </div>
    </div>
  </Teleport>
</template>
