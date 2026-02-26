<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRoute } from "vue-router";
import UiBadge from "../components/UiBadge.vue";
import UiButton from "../components/UiButton.vue";
import UiField from "../components/UiField.vue";
import UiInfoRow from "../components/UiInfoRow.vue";
import UiPanel from "../components/UiPanel.vue";
import {
  hiringDecisionLabel,
  hiringDecisionTone,
  interviewRecommendationLabel,
  interviewRecommendationTone,
  stageTone,
} from "../lib/status";
import { formatStageLabel } from "../lib/pipeline";
import { useRecruitingStore } from "../stores/recruiting";
import { useToastStore } from "../stores/toast";

const store = useRecruitingStore();
const toast = useToastStore();
const route = useRoute();

const selectedCandidateId = ref<number | null>(store.candidates[0]?.id ?? null);
const selectedJobId = ref<number | null>(null);
const finalDecision = ref<"HIRE" | "NO_HIRE">("HIRE");
const reasonCode = ref("skills_match");
const decisionNote = ref("");
const submitting = ref(false);

onMounted(() => {
  const candidateId = Number(route.query.candidateId);
  if (Number.isFinite(candidateId) && candidateId > 0) {
    selectedCandidateId.value = candidateId;
  }
  if (selectedCandidateId.value) {
    refreshContext().catch(() => undefined);
  }
});

const selectedCandidate = computed(() =>
  store.candidates.find((item) => item.id === selectedCandidateId.value) ?? null,
);

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

async function refreshContext() {
  if (!selectedCandidateId.value) {
    return;
  }
  await store.loadCandidateContext(selectedCandidateId.value);
}

watch(selectedCandidateId, (next) => {
  if (!next) {
    return;
  }
  refreshContext().catch(() => undefined);
});

async function submitDecision() {
  if (!selectedCandidateId.value) {
    toast.warning("请先选择候选人");
    return;
  }
  if (!reasonCode.value.trim()) {
    toast.warning("请填写决策原因码");
    return;
  }

  submitting.value = true;
  try {
    const record = await store.finalizeHiringDecision({
      candidate_id: selectedCandidateId.value,
      job_id: selectedJobId.value ?? undefined,
      final_decision: finalDecision.value,
      reason_code: reasonCode.value.trim(),
      note: decisionNote.value.trim() || undefined,
    });
    toast.success(`已提交最终决策：${hiringDecisionLabel(record.final_decision)}`);
  } finally {
    submitting.value = false;
  }
}
</script>

<template>
  <section class="flex flex-col gap-4">
    <UiPanel title="最终决策">
      <div class="grid grid-cols-3 gap-2.5 lt-lg:grid-cols-1">
        <UiField label="候选人">
          <select v-model="selectedCandidateId">
            <option :value="null" disabled>请选择候选人</option>
            <option v-for="candidate in store.candidates" :key="candidate.id" :value="candidate.id">
              #{{ candidate.id }} {{ candidate.name }}
            </option>
          </select>
        </UiField>
        <UiField label="岗位 ID（可选）" help="为空时按最近关联岗位推断">
          <input v-model.number="selectedJobId" type="number" min="1" placeholder="如 101" />
        </UiField>
        <UiField label="最终动作">
          <select v-model="finalDecision">
            <option value="HIRE">录用</option>
            <option value="NO_HIRE">遗憾</option>
          </select>
        </UiField>
      </div>

      <div class="grid grid-cols-2 gap-2.5 mt-2.5 lt-lg:grid-cols-1">
        <UiField label="原因码（必填）" help="用于后续复盘与统计">
          <input v-model="reasonCode" placeholder="例如：skills_match / culture_gap" />
        </UiField>
        <UiField label="备注（可选）">
          <input v-model="decisionNote" placeholder="补充说明" />
        </UiField>
      </div>

      <div class="flex items-center gap-2.5 mt-3 flex-wrap">
        <UiButton variant="secondary" :disabled="!selectedCandidateId" @click="refreshContext">刷新候选人上下文</UiButton>
        <UiButton :disabled="submitting || !selectedCandidateId" @click="submitDecision">
          {{ submitting ? "提交中..." : "提交最终决策" }}
        </UiButton>
      </div>
    </UiPanel>

    <UiPanel v-if="selectedCandidate" title="候选人上下文">
      <div class="grid grid-cols-2 gap-2.5 lt-lg:grid-cols-1">
        <UiInfoRow label="候选人" :value="`${selectedCandidate.name}（${selectedCandidate.years_of_experience}年）`" />
        <UiInfoRow label="当前阶段">
          <UiBadge :tone="stageTone(selectedCandidate.stage)">{{ formatStageLabel(selectedCandidate.stage) }}</UiBadge>
        </UiInfoRow>
      </div>
    </UiPanel>

    <UiPanel v-if="latestEvaluation" title="最新 AI 面后建议">
      <div class="flex items-center gap-2 mb-2">
        <UiBadge :tone="interviewRecommendationTone(latestEvaluation.recommendation)">
          {{ interviewRecommendationLabel(latestEvaluation.recommendation) }}
        </UiBadge>
        <span>综合分 {{ latestEvaluation.overall_score }}</span>
        <span class="text-muted">置信度 {{ latestEvaluation.confidence.toFixed(2) }}</span>
      </div>
      <UiInfoRow label="不确定性说明" :value="latestEvaluation.uncertainty" />
    </UiPanel>

    <UiPanel v-if="latestDecision" title="最新最终决策">
      <div class="flex items-center gap-2 mb-2 flex-wrap">
        <UiBadge :tone="hiringDecisionTone(latestDecision.final_decision)">
          {{ hiringDecisionLabel(latestDecision.final_decision) }}
        </UiBadge>
        <UiBadge
          :tone="latestDecision.ai_deviation ? 'warning' : 'success'"
        >
          {{ latestDecision.ai_deviation ? '与 AI 建议不一致' : '与 AI 建议一致' }}
        </UiBadge>
      </div>
      <div class="grid grid-cols-2 gap-2.5 lt-lg:grid-cols-1">
        <UiInfoRow label="AI 建议" :value="latestDecision.ai_recommendation || '无'" />
        <UiInfoRow label="原因码" :value="latestDecision.reason_code" />
        <UiInfoRow label="备注" :value="latestDecision.note || '-'" />
        <UiInfoRow label="提交时间" :value="latestDecision.updated_at" />
      </div>
    </UiPanel>
  </section>
</template>
