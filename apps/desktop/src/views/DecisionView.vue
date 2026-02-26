<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import type { CandidateRecord } from "@doss/shared";
import { useRoute } from "vue-router";
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
import {
  hiringDecisionLabel,
  hiringDecisionTone,
  interviewRecommendationLabel,
  interviewRecommendationTone,
  stageTone,
} from "../lib/status";
import { listDecisionCandidatesPage } from "../services/backend";
import { useRecruitingStore } from "../stores/recruiting";
import { useToastStore } from "../stores/toast";

const store = useRecruitingStore();
const toast = useToastStore();
const route = useRoute();

const loading = ref(false);
const page = ref(1);
const pageSize = ref(10);
const total = ref(0);
const rows = ref<CandidateRecord[]>([]);

const filters = reactive({
  jobId: 0,
  nameLike: "",
  interviewPassed: "" as "" | "pass" | "fail",
});

const drawerOpen = ref(false);
const drawerLoading = ref(false);
const selectedCandidateId = ref<number | null>(null);
const actingCandidateId = ref<number | null>(null);

const selectedCandidate = computed(() => {
  if (!selectedCandidateId.value) {
    return null;
  }
  return rows.value.find((item) => item.id === selectedCandidateId.value)
    ?? store.candidates.find((item) => item.id === selectedCandidateId.value)
    ?? null;
});

const latestEvaluation = computed(() => {
  if (!selectedCandidateId.value) {
    return null;
  }
  return store.interviewEvaluations[selectedCandidateId.value]?.[0] ?? null;
});

const latestDecision = computed(() => {
  if (!selectedCandidateId.value) {
    return null;
  }
  return store.hiringDecisions[selectedCandidateId.value]?.[0] ?? null;
});

const hasPrevPage = computed(() => page.value > 1);
const hasNextPage = computed(() => page.value * pageSize.value < total.value);
const totalPages = computed(() => Math.max(1, Math.ceil(total.value / pageSize.value)));

const jobOptions = computed(() => [
  { value: 0, label: "全部职位" },
  ...store.jobs.map((job) => ({ value: job.id, label: `${job.title} · ${job.company}` })),
]);

const interviewPassOptions = [
  { value: "", label: "全部" },
  { value: "pass", label: "面试通过" },
  { value: "fail", label: "面试未通过" },
];

function resolveErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  if (typeof error === "string" && error.trim()) {
    return error;
  }
  return fallback;
}

function isRowDecided(candidate: CandidateRecord): boolean {
  return candidate.stage === "OFFERED" || candidate.stage === "REJECTED";
}

async function loadRows() {
  loading.value = true;
  try {
    const data = await listDecisionCandidatesPage({
      page: page.value,
      page_size: pageSize.value,
      job_id: filters.jobId > 0 ? filters.jobId : undefined,
      name_like: filters.nameLike.trim() || undefined,
      interview_passed: filters.interviewPassed === "" ? undefined : filters.interviewPassed === "pass",
    });
    rows.value = data.items;
    total.value = data.total;
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "决策列表加载失败"));
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

async function openCandidateContext(candidate: CandidateRecord) {
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

async function submitDecision(candidate: CandidateRecord, finalDecision: "HIRE" | "NO_HIRE") {
  if (actingCandidateId.value) {
    return;
  }
  actingCandidateId.value = candidate.id;
  try {
    await store.finalizeHiringDecision({
      candidate_id: candidate.id,
      job_id: candidate.job_id ?? undefined,
      final_decision: finalDecision,
      reason_code: finalDecision === "HIRE" ? "interview_pass" : "interview_reject",
    });
    await loadRows();
    if (selectedCandidateId.value === candidate.id) {
      await store.loadCandidateContext(candidate.id);
    }
    toast.success(finalDecision === "HIRE" ? "已标记面试通过" : "已标记遗憾");
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "提交决策失败"));
  } finally {
    actingCandidateId.value = null;
  }
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
      await openCandidateContext(candidate);
    }
  }
});
</script>

<template>
  <section class="flex flex-col gap-4">
    <UiPanel title="已面试候选人列表">
      <div class="grid grid-cols-4 gap-2.5 mb-3 lt-lg:grid-cols-2 lt-sm:grid-cols-1">
        <UiField label="职位筛选">
          <UiSelect v-model="filters.jobId" :options="jobOptions" value-type="number" />
        </UiField>
        <UiField label="姓名关键词">
          <input v-model="filters.nameLike" placeholder="输入姓名关键词" @keyup.enter="applyFilters" />
        </UiField>
        <UiField label="面试是否通过">
          <UiSelect v-model="filters.interviewPassed" :options="interviewPassOptions" />
        </UiField>
        <div class="flex items-end gap-2">
          <UiButton variant="secondary" :disabled="loading" @click="applyFilters">查询</UiButton>
          <UiButton variant="ghost" :disabled="loading" @click="loadRows">刷新</UiButton>
        </div>
      </div>

      <UiTable>
        <thead>
          <tr>
            <UiTh>候选人</UiTh>
            <UiTh>职位</UiTh>
            <UiTh>阶段</UiTh>
            <UiTh>操作</UiTh>
          </tr>
        </thead>
        <tbody>
          <tr v-for="candidate in rows" :key="candidate.id">
            <UiTd>#{{ candidate.id }} {{ candidate.name }}</UiTd>
            <UiTd>{{ candidate.job_title || (candidate.job_id ? `职位 #${candidate.job_id}` : '-') }}</UiTd>
            <UiTd>
              <UiBadge :tone="stageTone(candidate.stage)">{{ formatStageLabel(candidate.stage) }}</UiBadge>
            </UiTd>
            <UiTd>
              <div class="flex items-center gap-2 flex-wrap">
                <UiButton variant="ghost" @click="openCandidateContext(candidate)">查看面试情况</UiButton>
                <UiButton
                  :disabled="isRowDecided(candidate) || actingCandidateId === candidate.id"
                  @click="submitDecision(candidate, 'HIRE')"
                >
                  面试通过
                </UiButton>
                <UiButton
                  variant="secondary"
                  :disabled="isRowDecided(candidate) || actingCandidateId === candidate.id"
                  @click="submitDecision(candidate, 'NO_HIRE')"
                >
                  遗憾
                </UiButton>
              </div>
            </UiTd>
          </tr>
          <tr v-if="!loading && rows.length === 0">
            <UiTd colspan="4" class="text-center text-muted py-6">暂无可决策候选人</UiTd>
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
          <h3 class="text-lg font-700">面试情况</h3>
          <UiButton variant="ghost" @click="drawerOpen = false">关闭</UiButton>
        </div>

        <UiPanel title="候选人信息">
          <div class="grid grid-cols-2 gap-2.5 lt-lg:grid-cols-1">
            <UiInfoRow label="候选人" :value="`${selectedCandidate.name}（${selectedCandidate.years_of_experience}年）`" />
            <UiInfoRow label="阶段">
              <UiBadge :tone="stageTone(selectedCandidate.stage)">{{ formatStageLabel(selectedCandidate.stage) }}</UiBadge>
            </UiInfoRow>
            <UiInfoRow label="职位" :value="selectedCandidate.job_title || (selectedCandidate.job_id ? `职位 #${selectedCandidate.job_id}` : '-')" />
            <UiInfoRow label="邮箱" :value="selectedCandidate.email_masked || '-'" />
          </div>
        </UiPanel>

        <UiPanel v-if="latestEvaluation" class="mt-3" title="最新 AI 面后建议">
          <div class="flex items-center gap-2 mb-2 flex-wrap">
            <UiBadge :tone="interviewRecommendationTone(latestEvaluation.recommendation)">
              {{ interviewRecommendationLabel(latestEvaluation.recommendation) }}
            </UiBadge>
            <span>综合分 {{ latestEvaluation.overall_score }}</span>
            <span class="text-muted">置信度 {{ latestEvaluation.confidence.toFixed(2) }}</span>
          </div>
          <UiInfoRow label="不确定性说明" :value="latestEvaluation.uncertainty" />
        </UiPanel>

        <UiPanel v-if="latestDecision" class="mt-3" title="最新最终决策">
          <div class="flex items-center gap-2 mb-2 flex-wrap">
            <UiBadge :tone="hiringDecisionTone(latestDecision.final_decision)">
              {{ hiringDecisionLabel(latestDecision.final_decision) }}
            </UiBadge>
            <UiBadge :tone="latestDecision.ai_deviation ? 'warning' : 'success'">
              {{ latestDecision.ai_deviation ? '与 AI 建议不一致' : '与 AI 建议一致' }}
            </UiBadge>
          </div>
          <div class="grid grid-cols-2 gap-2.5 lt-lg:grid-cols-1">
            <UiInfoRow label="AI 建议" :value="latestDecision.ai_recommendation || '-'" />
            <UiInfoRow label="原因码" :value="latestDecision.reason_code" />
            <UiInfoRow label="备注" :value="latestDecision.note || '-'" />
            <UiInfoRow label="提交时间" :value="latestDecision.updated_at" />
          </div>
        </UiPanel>

        <p v-if="drawerLoading" class="m-0 mt-3 text-sm text-muted">正在加载候选人上下文...</p>
      </aside>
    </div>
  </Teleport>
</template>
