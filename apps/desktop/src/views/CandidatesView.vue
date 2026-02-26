<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import type { CandidateRecord, PipelineStage } from "@doss/shared";
import { useRoute, useRouter } from "vue-router";
import UiBadge from "../components/UiBadge.vue";
import UiButton from "../components/UiButton.vue";
import UiField from "../components/UiField.vue";
import UiInfoRow from "../components/UiInfoRow.vue";
import UiPanel from "../components/UiPanel.vue";
import UiSelect from "../components/UiSelect.vue";
import UiTable from "../components/UiTable.vue";
import UiTd from "../components/UiTd.vue";
import UiTh from "../components/UiTh.vue";
import { formatStageLabel } from "../lib/pipeline";
import { stageTone } from "../lib/status";
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

const drawerOpen = ref(false);
const drawerLoading = ref(false);
const selectedCandidateId = ref<number | null>(null);
const actionLoading = ref(false);

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

const hasPrevPage = computed(() => page.value > 1);
const hasNextPage = computed(() => page.value * pageSize.value < total.value);
const totalPages = computed(() => Math.max(1, Math.ceil(total.value / pageSize.value)));

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
      sort_by: "job_title",
      sort_order: "asc",
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

function nextPage() {
  if (!hasNextPage.value) {
    return;
  }
  page.value += 1;
}

function prevPage() {
  if (!hasPrevPage.value) {
    return;
  }
  page.value -= 1;
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
    <UiPanel title="候选人列表">
      <div class="grid grid-cols-4 gap-2.5 mb-3 lt-lg:grid-cols-2 lt-sm:grid-cols-1">
        <UiField label="职位筛选">
          <UiSelect v-model="filters.jobId" :options="jobOptions" value-type="number" />
        </UiField>
        <UiField label="姓名关键词">
          <input v-model="filters.nameLike" placeholder="输入姓名关键词" @keyup.enter="applyFilters" />
        </UiField>
        <UiField label="阶段筛选">
          <UiSelect v-model="filters.stage" :options="stageOptions" />
        </UiField>
        <div class="flex items-end gap-2">
          <UiButton variant="secondary" :disabled="loading" @click="applyFilters">查询</UiButton>
          <UiButton variant="ghost" :disabled="loading" @click="loadRows">刷新</UiButton>
        </div>
      </div>

      <UiTable>
        <thead>
          <tr>
            <UiTh>姓名</UiTh>
            <UiTh>当前公司</UiTh>
            <UiTh>职位</UiTh>
            <UiTh>评分</UiTh>
            <UiTh>阶段</UiTh>
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
              <UiButton variant="ghost" @click="openDrawer(candidate)">查看详情</UiButton>
            </UiTd>
          </tr>
          <tr v-if="!loading && rows.length === 0">
            <UiTd colspan="6" class="text-center text-muted py-6">暂无数据</UiTd>
          </tr>
        </tbody>
      </UiTable>

      <div class="mt-3 flex items-center justify-between gap-2 flex-wrap">
        <span class="text-sm text-muted">第 {{ page }} / {{ totalPages }} 页，共 {{ total }} 条</span>
        <div class="flex items-center gap-2">
          <UiButton variant="ghost" :disabled="!hasPrevPage || loading" @click="prevPage">上一页</UiButton>
          <UiButton variant="ghost" :disabled="!hasNextPage || loading" @click="nextPage">下一页</UiButton>
        </div>
      </div>
    </UiPanel>
  </section>

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
</template>
