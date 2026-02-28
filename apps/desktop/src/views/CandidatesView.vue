<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from "vue";
import type { CandidateGender, CandidateRecord, PipelineStage, SortRule } from "@doss/shared";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useRoute, useRouter } from "vue-router";
import UiBadge from "../components/UiBadge.vue";
import UiButton from "../components/UiButton.vue";
import UiField from "../components/UiField.vue";
import UiPanel from "../components/UiPanel.vue";
import UiSelect from "../components/UiSelect.vue";
import UiTableFilterPanel from "../components/UiTableFilterPanel.vue";
import UiTable from "../components/UiTable.vue";
import UiTablePagination from "../components/UiTablePagination.vue";
import UiTd from "../components/UiTd.vue";
import UiTh from "../components/UiTh.vue";
import {
  ANALYSIS_PROGRESS_EVENT,
  appendAnalysisTrace,
  buildFallbackAnalysisMessage,
  resolveAnalysisStepIndex,
  shouldAcceptAnalysisProgressEvent,
  type AnalysisProgressEventPayload,
  type AnalysisProgressPhase,
  type AnalysisTraceItem,
} from "../lib/analysis-progress";
import { buildCandidateManualPayload } from "../lib/candidate-form";
import { formatStageLabel } from "../lib/pipeline";
import {
  resolveStructuredScoringViewModel,
} from "../lib/scoring-structured";
import { resolveScoringRerunFeedback } from "../lib/scoring-rerun-feedback";
import { stageTone } from "../lib/status";
import { normalizeSortRules } from "../lib/table-sort";
import { deleteResume, getResume, listCandidatesPage } from "../services/backend";
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

let filterNameLikeTimer: ReturnType<typeof setTimeout> | null = null;

const drawerOpen = ref(false);
const drawerLoading = ref(false);
const selectedCandidateId = ref<number | null>(null);
const actionLoading = ref(false);
const deletingCandidateId = ref<number | null>(null);
const deleteConfirmCandidate = ref<CandidateRecord | null>(null);
const createModalOpen = ref(false);
const creatingCandidate = ref(false);
const savingDetail = ref(false);
const createResumeFile = ref<File | null>(null);
const createResumeInput = ref<HTMLInputElement | null>(null);
const detailResumeFile = ref<File | null>(null);
const detailResumeInput = ref<HTMLInputElement | null>(null);
const detailResumeUploading = ref(false);
const detailResumeRemoving = ref(false);
const detailResumeEnableOcr = ref(false);
const detailResumeUploadTip = ref("");
const detailPersistedResumeFileName = ref("");
const resumeFileNameByCandidate = ref<Record<number, string>>({});
const analysisProgressVisible = ref(false);
const analysisProgressMinimized = ref(false);
const analysisProgressStepIndex = ref(0);
const analysisProgressStartedAt = ref(0);
const analysisProgressElapsedSeconds = ref(0);
const analysisRunId = ref("");
const analysisCurrentPhase = ref<AnalysisProgressPhase>("prepare");
const analysisTraceItems = ref<AnalysisTraceItem[]>([]);
const analysisUnlisten = ref<UnlistenFn | null>(null);
const analysisLastProgressEventAt = ref(0);
const analysisTraceListRef = ref<HTMLUListElement | null>(null);

const analysisProgressSteps = [
  {
    key: "prepare",
    label: "模板与上下文准备",
    description: "读取岗位模板与候选人上下文。",
  },
  {
    key: "t0",
    label: "T0 重要指标",
    description: "分析岗位描述/技能要求匹配度。",
  },
  {
    key: "t1",
    label: "T1 指标配置",
    description: "分析当前模板指标与候选人匹配度。",
  },
  {
    key: "t2",
    label: "T2 加分项",
    description: "分析候选人是否具备额外加分项。",
  },
  {
    key: "t3",
    label: "T3 风险项",
    description: "分析候选人风险项（低风险高分）。",
  },
  {
    key: "persist",
    label: "落库并刷新结果",
    description: "保存评分结果并刷新页面数据。",
  },
] as const;

const currentAnalysisStep = computed(() => {
  return analysisProgressSteps[Math.min(
    analysisProgressStepIndex.value,
    analysisProgressSteps.length - 1,
  )];
});

const latestAnalysisTraceMessage = computed(() => {
  if (!analysisTraceItems.value.length) {
    return "";
  }
  return analysisTraceItems.value[analysisTraceItems.value.length - 1]?.message ?? "";
});

const analysisTracePanelTitle = computed(() => {
  if (!analysisTraceItems.value.length) {
    return "正在准备评分过程...";
  }
  return `当前阶段：${currentAnalysisStep.value.label}`;
});

let analysisElapsedTimer: ReturnType<typeof setInterval> | null = null;
let analysisCloseTimer: ReturnType<typeof setTimeout> | null = null;
let analysisFallbackTimer: ReturnType<typeof setTimeout> | null = null;
let analysisFallbackInterval: ReturnType<typeof setInterval> | null = null;

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

const detailForm = reactive({
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
});

const selectedCandidate = computed(() => {
  if (!selectedCandidateId.value) {
    return null;
  }
  return rows.value.find((item) => item.id === selectedCandidateId.value)
    ?? store.candidates.find((item) => item.id === selectedCandidateId.value)
    ?? null;
});

const selectedScoring = computed(() => {
  if (!selectedCandidateId.value) {
    return [];
  }
  return store.scoringResults[selectedCandidateId.value] ?? [];
});

const selectedEvents = computed(() => {
  if (!selectedCandidateId.value) {
    return [];
  }
  return store.pipelineEvents[selectedCandidateId.value] ?? [];
});

const selectedStructuredAssessment = computed(() =>
  resolveStructuredScoringViewModel(selectedScoring.value[0]),
);

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
const hasDetailResumeSelection = computed(() => Boolean(detailResumeFile.value?.name));
const hasDetailPersistedResume = computed(() => Boolean(detailPersistedResumeFileName.value));
const detailResumeFileName = computed(() => detailResumeFile.value?.name || detailPersistedResumeFileName.value);
const detailResumeFileLabel = computed(() => (hasDetailResumeSelection.value ? "待上传文件" : "已上传文件"));

function formatScore5(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) {
    return "-";
  }
  return Number(value).toFixed(2);
}

function screeningRecommendationLabel(recommendation: "PASS" | "REVIEW" | "REJECT"): string {
  if (recommendation === "PASS") {
    return "通过";
  }
  if (recommendation === "REVIEW") {
    return "建议复核";
  }
  return "不通过";
}

function screeningRiskLabel(level: "LOW" | "MEDIUM" | "HIGH"): string {
  if (level === "HIGH") {
    return "高风险";
  }
  if (level === "MEDIUM") {
    return "中风险";
  }
  return "低风险";
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

function formatTraceTime(value: string): string {
  const parsed = Date.parse(value);
  if (!Number.isFinite(parsed)) {
    return value;
  }
  return new Date(parsed).toLocaleTimeString("zh-CN", {
    hour12: false,
  });
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

function reloadRowsFromFilters() {
  if (page.value !== 1) {
    page.value = 1;
    return;
  }
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

function onDetailResumeChange(event: Event) {
  const target = event.target as HTMLInputElement | null;
  const file = target?.files?.[0];
  detailResumeFile.value = file ?? null;
  detailResumeUploadTip.value = "";
  if (file) {
    void uploadDetailResume(file);
  }
}

function clearDetailResume() {
  detailResumeFile.value = null;
  detailResumeEnableOcr.value = false;
  detailResumeUploadTip.value = "";
  if (detailResumeInput.value) {
    detailResumeInput.value.value = "";
  }
}

function openDetailResumePicker() {
  if (savingDetail.value || actionLoading.value || detailResumeUploading.value || detailResumeRemoving.value) {
    return;
  }
  detailResumeInput.value?.click();
}

async function loadDetailResumeSnapshot(candidateId: number) {
  const resume = await getResume(candidateId);
  const fileName = (resume?.original_file_name ?? "").trim();
  if (fileName) {
    resumeFileNameByCandidate.value[candidateId] = fileName;
  } else {
    delete resumeFileNameByCandidate.value[candidateId];
  }
  if (selectedCandidateId.value === candidateId) {
    detailPersistedResumeFileName.value = fileName;
  }
}

function fillDetailForm(candidate: CandidateRecord) {
  detailForm.name = candidate.name;
  detailForm.currentCompany = candidate.current_company || "";
  detailForm.jobId = candidate.job_id ?? 0;
  detailForm.yearsOfExperience = String(candidate.years_of_experience ?? 0);
  detailForm.score = candidate.score === null || candidate.score === undefined ? "" : String(candidate.score);
  detailForm.age = candidate.age === null || candidate.age === undefined ? "" : String(candidate.age);
  detailForm.gender = candidate.gender ?? "";
  detailForm.address = candidate.address || "";
  detailForm.phone = "";
  detailForm.email = "";
  detailForm.tagsText = candidate.tags.join(", ");
}

function cleanupAnalysisProgressTimers() {
  if (analysisElapsedTimer) {
    clearInterval(analysisElapsedTimer);
    analysisElapsedTimer = null;
  }
  if (analysisCloseTimer) {
    clearTimeout(analysisCloseTimer);
    analysisCloseTimer = null;
  }
  if (analysisFallbackTimer) {
    clearTimeout(analysisFallbackTimer);
    analysisFallbackTimer = null;
  }
  if (analysisFallbackInterval) {
    clearInterval(analysisFallbackInterval);
    analysisFallbackInterval = null;
  }
}

function teardownAnalysisProgressListener() {
  if (analysisUnlisten.value) {
    analysisUnlisten.value();
    analysisUnlisten.value = null;
  }
}

function addAnalysisTrace(payload: AnalysisProgressEventPayload) {
  analysisTraceItems.value = appendAnalysisTrace(analysisTraceItems.value, payload, 30);
  void scrollAnalysisTraceToBottom();
}

async function scrollAnalysisTraceToBottom() {
  await nextTick();
  requestAnimationFrame(() => {
    const container = analysisTraceListRef.value;
    if (!container) {
      return;
    }
    container.scrollTop = container.scrollHeight;
  });
}

function scheduleAnalysisFallbackProgress() {
  if (!analysisProgressVisible.value) {
    return;
  }
  analysisFallbackTimer = setTimeout(() => {
    analysisFallbackInterval = setInterval(() => {
      if (!analysisProgressVisible.value) {
        return;
      }
      if (Date.now() - analysisLastProgressEventAt.value <= 1500) {
        return;
      }
      const phase = analysisCurrentPhase.value;
      addAnalysisTrace({
        runId: analysisRunId.value,
        candidateId: selectedCandidate.value?.id ?? 0,
        phase,
        status: "running",
        kind: "progress",
        message: buildFallbackAnalysisMessage(phase),
        at: new Date().toISOString(),
      });
    }, 1300);
  }, 1500);
}

async function setupAnalysisProgressListener(runId: string, candidateId: number) {
  teardownAnalysisProgressListener();
  analysisUnlisten.value = await listen<AnalysisProgressEventPayload>(
    ANALYSIS_PROGRESS_EVENT,
    (event) => {
      const payload = event.payload;
      if (!shouldAcceptAnalysisProgressEvent(payload, runId, candidateId)) {
        return;
      }
      analysisLastProgressEventAt.value = Date.now();
      analysisCurrentPhase.value = payload.phase;
      analysisProgressStepIndex.value = resolveAnalysisStepIndex(
        analysisProgressStepIndex.value,
        payload.phase,
        payload.status,
      );
      addAnalysisTrace(payload);
    },
  );
}

function closeAnalysisProgress() {
  cleanupAnalysisProgressTimers();
  teardownAnalysisProgressListener();
  analysisProgressVisible.value = false;
  analysisProgressMinimized.value = false;
  analysisProgressStepIndex.value = 0;
  analysisProgressElapsedSeconds.value = 0;
  analysisProgressStartedAt.value = 0;
  analysisCurrentPhase.value = "prepare";
  analysisTraceItems.value = [];
  analysisLastProgressEventAt.value = 0;
  analysisRunId.value = "";
}

function startAnalysisProgress(runId: string, candidateId: number) {
  closeAnalysisProgress();
  analysisProgressVisible.value = true;
  analysisRunId.value = runId;
  analysisProgressStartedAt.value = Date.now();
  analysisProgressStepIndex.value = 0;
  analysisProgressElapsedSeconds.value = 0;
  analysisCurrentPhase.value = "prepare";
  analysisLastProgressEventAt.value = Date.now();
  addAnalysisTrace({
    runId,
    candidateId,
    phase: "prepare",
    status: "running",
    kind: "start",
    message: "已开始重新评分，正在准备模板与候选人上下文",
    at: new Date().toISOString(),
  });

  analysisElapsedTimer = setInterval(() => {
    if (!analysisProgressStartedAt.value) {
      return;
    }
    const elapsed = Math.max(0, Math.floor((Date.now() - analysisProgressStartedAt.value) / 1000));
    analysisProgressElapsedSeconds.value = elapsed;
  }, 1000);
  scheduleAnalysisFallbackProgress();
}

function finishAnalysisProgress(status: "completed" | "failed", message: string) {
  cleanupAnalysisProgressTimers();
  analysisProgressStepIndex.value = resolveAnalysisStepIndex(
    analysisProgressStepIndex.value,
    status === "completed" ? "persist" : analysisCurrentPhase.value,
    status,
  );
  addAnalysisTrace({
    runId: analysisRunId.value,
    candidateId: selectedCandidate.value?.id ?? 0,
    phase: status === "completed" ? "persist" : analysisCurrentPhase.value,
    status,
    kind: "end",
    message,
    at: new Date().toISOString(),
  });
  analysisCloseTimer = setTimeout(() => {
    closeAnalysisProgress();
  }, status === "completed" ? 300 : 800);
}

function minimizeAnalysisProgress() {
  if (!analysisProgressVisible.value) {
    return;
  }
  analysisProgressMinimized.value = true;
}

function restoreAnalysisProgress() {
  if (!analysisProgressVisible.value) {
    return;
  }
  analysisProgressMinimized.value = false;
  void scrollAnalysisTraceToBottom();
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
        await store.importResumeFile({
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

    toast.success(resumeFile ? "候选人和简历已保存（未自动分析）" : "候选人已创建");
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "创建候选人失败"));
  } finally {
    creatingCandidate.value = false;
  }
}

async function openDrawer(candidate: CandidateRecord) {
  selectedCandidateId.value = candidate.id;
  fillDetailForm(candidate);
  detailPersistedResumeFileName.value = resumeFileNameByCandidate.value[candidate.id] ?? "";
  drawerOpen.value = true;
  drawerLoading.value = true;
  try {
    const [contextResult] = await Promise.allSettled([
      store.loadCandidateContext(candidate.id),
      loadDetailResumeSnapshot(candidate.id),
    ]);
    if (contextResult.status === "rejected") {
      throw contextResult.reason;
    }
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
    delete resumeFileNameByCandidate.value[candidate.id];
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

async function saveCandidateDetail() {
  if (!selectedCandidate.value || savingDetail.value) {
    return;
  }
  const candidateId = selectedCandidate.value.id;

  const built = buildCandidateManualPayload({
    name: detailForm.name,
    currentCompany: detailForm.currentCompany,
    jobId: detailForm.jobId,
    yearsOfExperience: detailForm.yearsOfExperience,
    score: detailForm.score,
    age: detailForm.age,
    gender: detailForm.gender,
    address: detailForm.address,
    phone: detailForm.phone,
    email: detailForm.email,
    tagsText: detailForm.tagsText,
  });
  if (!built.ok) {
    toast.warning(built.error);
    return;
  }

  savingDetail.value = true;
  try {
    await store.updateCandidate({
      candidate_id: candidateId,
      ...built.payload,
      job_id: detailForm.jobId > 0 ? detailForm.jobId : null,
    });
    await Promise.all([loadRows(), store.loadCandidateContext(candidateId)]);
    const refreshed = rows.value.find((item) => item.id === candidateId)
      ?? store.candidates.find((item) => item.id === candidateId);
    if (refreshed) {
      fillDetailForm(refreshed);
    }
    toast.success("候选人信息已更新");
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "保存候选人信息失败"));
  } finally {
    savingDetail.value = false;
  }
}

async function uploadDetailResume(file?: File) {
  const targetFile = file ?? detailResumeFile.value;
  if (!selectedCandidate.value || !targetFile || detailResumeUploading.value || detailResumeRemoving.value) {
    return;
  }
  const candidateId = selectedCandidate.value.id;
  const jobId = selectedCandidate.value.job_id ?? undefined;
  detailResumeUploading.value = true;
  detailResumeUploadTip.value = "正在上传简历...";
  try {
    await store.importResumeFile({
      candidateId,
      file: targetFile,
      enableOcr: detailResumeEnableOcr.value,
      jobId,
    });
    const uploadedName = targetFile.name.trim();
    if (uploadedName) {
      resumeFileNameByCandidate.value[candidateId] = uploadedName;
      detailPersistedResumeFileName.value = uploadedName;
    }
    await Promise.allSettled([
      store.loadCandidateContext(candidateId),
      loadDetailResumeSnapshot(candidateId),
    ]);
    detailResumeUploadTip.value = "简历已上传";
    toast.success("简历上传成功");
  } catch (error) {
    detailResumeUploadTip.value = "上传失败，请重新选择";
    toast.danger(resolveErrorMessage(error, "简历上传失败"));
  } finally {
    detailResumeUploading.value = false;
  }
}

async function removeDetailResume() {
  if (!selectedCandidate.value || detailResumeUploading.value || detailResumeRemoving.value) {
    return;
  }
  const candidateId = selectedCandidate.value.id;
  detailResumeRemoving.value = true;
  detailResumeUploadTip.value = "正在移除简历...";
  try {
    const removeFromStore = (store as unknown as {
      removeResume?: (id: number) => Promise<boolean>;
    }).removeResume;
    const removed = typeof removeFromStore === "function"
      ? await removeFromStore(candidateId)
      : await deleteResume(candidateId);
    if (typeof removeFromStore !== "function") {
      await store.refreshMetrics();
    }
    detailPersistedResumeFileName.value = "";
    delete resumeFileNameByCandidate.value[candidateId];
    await Promise.allSettled([
      store.loadCandidateContext(candidateId),
      loadDetailResumeSnapshot(candidateId),
    ]);
    detailResumeUploadTip.value = removed ? "简历已移除" : "当前无已上传简历";
    if (removed) {
      toast.success("简历已移除");
    } else {
      toast.warning("当前无已上传简历");
    }
  } catch (error) {
    detailResumeUploadTip.value = "移除失败，请重试";
    toast.danger(resolveErrorMessage(error, "移除简历失败"));
  } finally {
    detailResumeRemoving.value = false;
  }
}

async function rerunScoring() {
  if (!selectedCandidate.value || actionLoading.value) {
    return;
  }
  const candidateId = selectedCandidate.value.id;
  const jobId = selectedCandidate.value.job_id ?? undefined;
  const runId = `scoring-${candidateId}-${Date.now()}`;
  actionLoading.value = true;
  startAnalysisProgress(runId, candidateId);
  try {
    await setupAnalysisProgressListener(runId, candidateId);
    await store.runScoring(candidateId, jobId, runId);
    await store.loadCandidateContext(candidateId);
    finishAnalysisProgress("completed", "评分完成并已刷新结果");
    toast.success("评分结果已更新");
  } catch (error) {
    const feedback = resolveScoringRerunFeedback(error, "重新分析失败");
    finishAnalysisProgress("failed", feedback.message);
    if (feedback.tone === "warning") {
      toast.warning(feedback.message);
      return;
    }
    toast.danger(feedback.message);
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

function isAnalysisStepCompleted(index: number) {
  return analysisProgressStepIndex.value > index;
}

function isAnalysisStepActive(index: number) {
  return analysisProgressStepIndex.value === index;
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

watch(() => filters.jobId, () => {
  reloadRowsFromFilters();
});

watch(() => filters.stage, () => {
  reloadRowsFromFilters();
});

watch(() => filters.nameLike, () => {
  if (filterNameLikeTimer) {
    clearTimeout(filterNameLikeTimer);
  }
  filterNameLikeTimer = setTimeout(() => {
    reloadRowsFromFilters();
  }, 250);
});

watch(drawerOpen, (open) => {
  if (!open) {
    clearDetailResume();
    detailPersistedResumeFileName.value = "";
    detailResumeRemoving.value = false;
  }
});

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

onUnmounted(() => {
  if (filterNameLikeTimer) {
    clearTimeout(filterNameLikeTimer);
  }
  closeAnalysisProgress();
});
</script>

<template>
  <section class="flex flex-col gap-4">
    <header class="flex items-center gap-3">
      <h2 class="text-2xl font-700">候选人池</h2>
    </header>

    <UiPanel>
      <template #header>
        <div class="mb-1 flex items-center justify-between gap-3 flex-wrap">
          <input
            v-model="filters.nameLike"
            class="candidates-header-input w-full max-w-80 lt-sm:max-w-full"
            placeholder="输入姓名关键词"
            :disabled="loading"
          />
          <UiButton :disabled="loading" @click="openCreateCandidateModal">创建候选人</UiButton>
        </div>
      </template>

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
            <UiTd>{{ candidate.name }}</UiTd>
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
            <UiField label="姓名">
              <input v-model="detailForm.name" :disabled="savingDetail" placeholder="例如：张三" />
            </UiField>
            <UiField label="绑定职位">
              <UiSelect
                v-model="detailForm.jobId"
                :disabled="savingDetail"
                :options="createJobOptions"
                value-type="number"
              />
            </UiField>
            <UiField label="当前公司">
              <input v-model="detailForm.currentCompany" :disabled="savingDetail" placeholder="当前任职公司" />
            </UiField>
            <UiField label="工作年限（年）">
              <input
                v-model="detailForm.yearsOfExperience"
                :disabled="savingDetail"
                type="number"
                min="0"
                step="0.5"
                placeholder="例如：5"
              />
            </UiField>
            <UiField label="评分（0-100）">
              <input
                v-model="detailForm.score"
                :disabled="savingDetail"
                type="number"
                min="0"
                max="100"
                step="0.1"
                placeholder="可选"
              />
            </UiField>
            <UiField label="年龄">
              <input
                v-model="detailForm.age"
                :disabled="savingDetail"
                type="number"
                min="0"
                step="1"
                placeholder="可选"
              />
            </UiField>
            <UiField label="性别">
              <UiSelect v-model="detailForm.gender" :disabled="savingDetail" :options="genderOptions" />
            </UiField>
            <UiField label="地址">
              <input v-model="detailForm.address" :disabled="savingDetail" placeholder="可选" />
            </UiField>
            <UiField label="电话" help="留空则保持原值">
              <input
                v-model="detailForm.phone"
                :disabled="savingDetail"
                :placeholder="selectedCandidate.phone_masked || '输入新手机号'"
              />
            </UiField>
            <UiField label="邮箱" help="留空则保持原值">
              <input
                v-model="detailForm.email"
                :disabled="savingDetail"
                :placeholder="selectedCandidate.email_masked || '输入新邮箱'"
              />
            </UiField>
            <UiField class="col-span-2 lt-lg:col-span-1" label="标签" help="多个标签可用英文逗号、中文逗号或换行分隔">
              <input v-model="detailForm.tagsText" :disabled="savingDetail" placeholder="例如：Vue, TypeScript, 稳定" />
            </UiField>
            <UiField class="col-span-2 lt-lg:col-span-1" label="简历上传" help="支持 .pdf .docx .txt .md 以及图片格式">
              <div class="relative">
                <input
                  ref="detailResumeInput"
                  type="file"
                  :accept="resumeAccept"
                  :disabled="savingDetail || actionLoading || detailResumeUploading || detailResumeRemoving"
                  class="pointer-events-none absolute h-0 w-0 opacity-0"
                  @change="onDetailResumeChange"
                />
                <div class="flex items-center gap-2 rounded-xl border border-line bg-card px-2 py-1.5">
                  <button
                    type="button"
                    class="shrink-0 rounded-lg border border-line bg-base px-3 py-1.5 text-sm text-fg hover:bg-card disabled:cursor-not-allowed disabled:opacity-60"
                    :disabled="savingDetail || actionLoading || detailResumeUploading || detailResumeRemoving"
                    @click="openDetailResumePicker"
                  >
                    选择文件
                  </button>
                  <input
                    type="text"
                    readonly
                    :value="detailResumeFileName || '未选择文件'"
                    class="min-w-0 flex-1 border-none bg-transparent px-1 py-1 text-sm text-muted outline-none"
                  />
                  <button
                    v-if="hasDetailResumeSelection"
                    type="button"
                    class="h-5 w-5 shrink-0 rounded-full border border-line bg-card text-xs text-muted hover:text-fg"
                    :disabled="savingDetail || actionLoading || detailResumeUploading || detailResumeRemoving"
                    @click="clearDetailResume"
                  >
                    ×
                  </button>
                </div>
              </div>
              <label class="mt-2 inline-flex items-center gap-2 text-xs text-muted">
                <input
                  v-model="detailResumeEnableOcr"
                  type="checkbox"
                  :disabled="savingDetail || actionLoading || detailResumeUploading || detailResumeRemoving"
                />
                <span>文本为空时启用 OCR（适用于扫描件）</span>
              </label>
              <div v-if="hasDetailPersistedResume" class="mt-2 flex items-center gap-2">
                <UiButton
                  variant="ghost"
                  type="button"
                  :disabled="savingDetail || actionLoading || detailResumeUploading || detailResumeRemoving"
                  @click="removeDetailResume"
                >
                  {{ detailResumeRemoving ? "移除中..." : "移除已上传简历" }}
                </UiButton>
              </div>
              <p v-if="detailResumeFileName" class="m-0 mt-2 text-xs text-muted truncate">
                {{ detailResumeFileLabel }}：{{ detailResumeFileName }}
              </p>
              <p v-if="hasDetailPersistedResume && !hasDetailResumeSelection" class="m-0 mt-1 text-xs text-muted">
                重新选择文件并上传将覆盖当前简历
              </p>
              <p v-if="detailResumeUploadTip" class="m-0 mt-1 text-xs text-muted">
                {{ detailResumeUploadTip }}
              </p>
            </UiField>
          </div>

          <div class="mt-3 flex flex-wrap gap-2">
            <UiButton :disabled="savingDetail || actionLoading" @click="saveCandidateDetail">
              {{ savingDetail ? "保存中..." : "保存修改" }}
            </UiButton>
            <UiButton :disabled="savingDetail || actionLoading" @click="rerunScoring">重新分析</UiButton>
            <UiButton variant="secondary" :disabled="savingDetail || actionLoading" @click="goInterview">邀约面试</UiButton>
            <UiButton variant="ghost" :disabled="savingDetail || actionLoading" @click="rejectCandidate">遗憾</UiButton>
          </div>
        </UiPanel>

        <UiPanel v-if="selectedScoring.length && selectedStructuredAssessment" class="mt-3" title="AI评估">
          <div class="flex items-center gap-2 mb-2 flex-wrap">
            <UiBadge :tone="screeningTone(selectedScoring[0].recommendation)">
              {{ screeningRecommendationLabel(selectedScoring[0].recommendation) }}
            </UiBadge>
            <UiBadge :tone="selectedScoring[0].risk_level === 'HIGH' ? 'danger' : selectedScoring[0].risk_level === 'MEDIUM' ? 'warning' : 'info'">
              {{ screeningRiskLabel(selectedScoring[0].risk_level) }}
            </UiBadge>
            <p class="m-0 text-xs text-muted">模板：{{ selectedStructuredAssessment.templateName }}</p>
          </div>

          <div class="rounded-xl border border-line bg-card/70 p-3">
            <div class="flex items-start justify-between gap-3 flex-wrap">
              <div>
                <p class="m-0 text-sm text-muted">整体评分（5分制）</p>
                <p class="m-0 mt-1 text-2xl font-700">
                  {{ formatScore5(selectedStructuredAssessment.overallScore5) }}
                  <span class="text-base font-500 text-muted">/ 5.00</span>
                </p>
                <p class="m-0 mt-1 text-xs text-muted">
                  T0 {{ selectedStructuredAssessment.weights.t0 }}% ·
                  T1 {{ selectedStructuredAssessment.weights.t1 }}% ·
                  T2 {{ selectedStructuredAssessment.weights.t2 }}% ·
                  T3 {{ selectedStructuredAssessment.weights.t3 }}%
                </p>
              </div>
              <div class="grid grid-cols-2 gap-x-3 gap-y-1 text-sm min-w-[280px] lt-sm:min-w-0">
                <p class="m-0">T0：{{ formatScore5(selectedStructuredAssessment.subscores.t0) }}</p>
                <p class="m-0">T1：{{ formatScore5(selectedStructuredAssessment.subscores.t1) }}</p>
                <p class="m-0">T2：{{ formatScore5(selectedStructuredAssessment.subscores.t2) }}</p>
                <p class="m-0">T3：{{ formatScore5(selectedStructuredAssessment.subscores.t3) }}</p>
              </div>
            </div>
            <div class="mt-3 border-t border-line pt-2">
              <p class="m-0 text-sm font-700">整体总结</p>
              <p class="m-0 mt-1 text-sm leading-6">{{ selectedStructuredAssessment.overallComment }}</p>
            </div>
          </div>

          <div
            v-for="module in selectedStructuredAssessment.modules"
            :key="module.key"
            class="mt-3 rounded-xl border border-line bg-card/70 p-3"
          >
            <div class="flex items-center justify-between gap-2 flex-wrap">
              <p class="m-0 text-sm font-700">{{ module.title }}</p>
              <div class="flex items-center gap-2">
                <span class="text-xs text-muted">权重 {{ module.weight }}%</span>
                <span class="text-xs text-muted">模块分 {{ formatScore5(module.score5) }} / 5</span>
              </div>
            </div>

            <p class="m-0 mt-2 text-sm text-muted leading-6">{{ module.comment }}</p>

            <div class="mt-2 overflow-x-auto">
              <table class="w-full border-collapse text-sm">
                <thead>
                  <tr class="text-left text-muted">
                    <th class="px-2 py-1.5 font-600">指标 + 权重</th>
                    <th class="px-2 py-1.5 font-600">候选人得分</th>
                  </tr>
                </thead>
                <tbody>
                  <tr
                    v-for="item in module.items"
                    :key="`${module.key}-${item.key}`"
                    class="border-t border-line align-top"
                  >
                    <td class="px-2 py-2">
                      <p class="m-0 font-600">{{ item.label }}（{{ item.weight }}%）</p>
                      <p class="m-0 mt-1 text-xs text-muted leading-5">{{ item.reason }}</p>
                    </td>
                    <td class="px-2 py-2 whitespace-nowrap">
                      <p class="m-0">{{ formatScore5(item.score5) }} / 5</p>
                    </td>
                  </tr>
                  <tr v-if="module.items.length === 0">
                    <td colspan="2" class="px-2 py-3 text-center text-muted">暂无模块指标结果</td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>

          <div v-if="selectedStructuredAssessment.riskAlerts.length" class="mt-3 rounded-xl border border-line p-3">
            <p class="m-0 text-sm font-700">风险提示</p>
            <ul class="m-0 mt-1 list-disc pl-4 text-sm text-muted leading-6">
              <li v-for="item in selectedStructuredAssessment.riskAlerts" :key="item">{{ item }}</li>
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
      v-if="analysisProgressVisible && !analysisProgressMinimized"
      class="fixed inset-0 z-[87] flex items-center justify-center bg-black/35 p-4"
    >
      <div class="w-full max-w-3xl">
        <UiPanel title="AI评分中">
          <div class="mt-1 flex items-center gap-2 overflow-x-auto pb-1">
            <template v-for="(step, index) in analysisProgressSteps" :key="step.key">
              <div class="flex min-w-[180px] items-center gap-2">
                <span
                  class="inline-flex h-6 w-6 items-center justify-center rounded-full border text-xs font-700"
                  :class="isAnalysisStepActive(index)
                    ? 'border-brand bg-brand/14 text-fg'
                    : isAnalysisStepCompleted(index)
                      ? 'border-brand/50 bg-brand/12 text-fg'
                      : 'border-line bg-card text-muted'"
                >
                  {{ index + 1 }}
                </span>
                <div class="min-w-0">
                  <p class="m-0 text-sm font-600">{{ step.label }}</p>
                  <p class="m-0 text-xs text-muted">
                    {{ isAnalysisStepActive(index) ? "进行中" : isAnalysisStepCompleted(index) ? "已完成" : "待处理" }}
                  </p>
                </div>
              </div>
              <div
                v-if="index < analysisProgressSteps.length - 1"
                class="h-[2px] min-w-8 flex-1 rounded-full"
                :class="isAnalysisStepCompleted(index) ? 'bg-brand/45' : 'bg-line'"
              />
            </template>
          </div>

          <div class="mt-3 flex items-center gap-2.5">
            <span class="h-4 w-4 rounded-full border-2 border-brand/28 border-t-brand animate-spin" />
            <p class="m-0 text-sm font-600">{{ currentAnalysisStep.label }}</p>
          </div>
          <p class="m-0 mt-1 text-xs text-muted">预计需数秒，请勿关闭页面</p>

          <div class="mt-3 rounded-xl border border-line bg-card/70 px-3 py-2.5">
            <p class="m-0 text-sm font-600">{{ analysisTracePanelTitle }}</p>
            <ul
              ref="analysisTraceListRef"
              class="m-0 mt-2 max-h-44 list-none overflow-y-auto p-0 flex flex-col gap-1.5"
            >
              <li
                v-for="item in analysisTraceItems"
                :key="item.id"
                class="rounded-lg border px-2 py-1.5"
                :class="item.status === 'failed'
                  ? 'border-danger/38 bg-danger/10'
                  : item.status === 'completed'
                    ? 'border-brand/36 bg-brand/10'
                    : 'border-line bg-card'"
              >
                <p class="m-0 text-[0.82rem] leading-5">{{ item.message }}</p>
                <p class="m-0 mt-0.5 text-[0.72rem] text-muted">
                  {{ formatTraceTime(item.at) }} · {{ item.phase }}
                </p>
              </li>
            </ul>
          </div>

          <div class="mt-3 flex items-center justify-between gap-2">
            <span class="text-xs text-muted">已耗时 {{ analysisProgressElapsedSeconds }}s</span>
            <UiButton variant="ghost" :disabled="!analysisProgressVisible" @click="minimizeAnalysisProgress">最小化</UiButton>
          </div>
        </UiPanel>
      </div>
    </div>
  </Teleport>

  <Teleport to="body">
    <div
      v-if="analysisProgressVisible && analysisProgressMinimized"
      class="fixed right-4 bottom-4 z-[87] w-[320px] max-w-[calc(100vw-2rem)]"
    >
      <UiPanel>
        <div class="flex items-start justify-between gap-2">
          <div class="min-w-0">
            <p class="m-0 text-sm font-600">AI分析进行中</p>
            <p class="m-0 mt-1 text-xs text-muted truncate">
              {{ currentAnalysisStep.label }} · {{ latestAnalysisTraceMessage || `${analysisProgressElapsedSeconds}s` }}
            </p>
          </div>
          <span class="mt-0.5 h-3.5 w-3.5 rounded-full border-2 border-brand/28 border-t-brand animate-spin" />
        </div>
        <div class="mt-2 flex justify-end">
          <UiButton variant="ghost" @click="restoreAnalysisProgress">展开</UiButton>
        </div>
      </UiPanel>
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

<style scoped>
.candidates-header-input {
  min-height: 40px;
  padding-top: 8px;
  padding-bottom: 8px;
}
</style>
