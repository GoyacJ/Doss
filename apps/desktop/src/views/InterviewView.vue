<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { useRoute } from "vue-router";
import { formatStageLabel } from "../lib/pipeline";
import {
  interviewRecommendationLabel,
  interviewRecommendationTone,
  stageTone,
} from "../lib/status";
import UiBadge from "../components/UiBadge.vue";
import UiButton from "../components/UiButton.vue";
import UiField from "../components/UiField.vue";
import UiInfoRow from "../components/UiInfoRow.vue";
import UiPanel from "../components/UiPanel.vue";
import UiTable from "../components/UiTable.vue";
import UiTd from "../components/UiTd.vue";
import UiTh from "../components/UiTh.vue";
import { useRecruitingStore } from "../stores/recruiting";
import { useToastStore } from "../stores/toast";

interface InterviewQuestionDraft {
  primaryQuestion: string;
  followUpsText: string;
  scoringPointsText: string;
  redFlagsText: string;
}

const store = useRecruitingStore();
const toast = useToastStore();
const route = useRoute();

const selectedCandidateId = ref<number | null>(store.candidates[0]?.id ?? null);
const selectedJobId = ref<number | null>(null);

const generating = ref(false);
const savingKit = ref(false);
const submitting = ref(false);
const evaluating = ref(false);

const questionDrafts = ref<InterviewQuestionDraft[]>([]);
const transcriptText = ref("");
const feedbackSummary = ref("");
const feedbackRedFlags = ref("");
const recordingPath = ref("");
const feedbackScores = reactive({
  communication: 3,
  problem_solving: 3,
  execution: 3,
  values_fit: 3,
});

onMounted(() => {
  const candidateId = Number(route.query.candidateId);
  if (Number.isFinite(candidateId) && candidateId > 0) {
    selectedCandidateId.value = candidateId;
  }
});

const selectedCandidate = computed(() =>
  store.candidates.find((item) => item.id === selectedCandidateId.value) ?? null,
);

const latestScreening = computed(() => {
  if (!selectedCandidateId.value) {
    return null;
  }
  return store.screeningResults[selectedCandidateId.value]?.[0] ?? null;
});

const latestEvaluation = computed(() => {
  if (!selectedCandidateId.value) {
    return null;
  }
  return store.interviewEvaluations[selectedCandidateId.value]?.[0] ?? null;
});

const latestFeedback = computed(() => {
  if (!selectedCandidateId.value) {
    return null;
  }
  return store.interviewFeedback[selectedCandidateId.value]?.[0] ?? null;
});

function parseLineList(text: string): string[] {
  return text
    .split(/\n+/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function fromQuestion(question: {
  primary_question: string;
  follow_ups: string[];
  scoring_points: string[];
  red_flags: string[];
}): InterviewQuestionDraft {
  return {
    primaryQuestion: question.primary_question,
    followUpsText: question.follow_ups.join("\n"),
    scoringPointsText: question.scoring_points.join("\n"),
    redFlagsText: question.red_flags.join("\n"),
  };
}

function createDraft(): InterviewQuestionDraft {
  return {
    primaryQuestion: "",
    followUpsText: "",
    scoringPointsText: "",
    redFlagsText: "",
  };
}

async function loadCandidateContext() {
  if (!selectedCandidateId.value) {
    return;
  }

  await store.loadCandidateContext(selectedCandidateId.value);
}

async function generateKit() {
  if (!selectedCandidateId.value) {
    toast.warning("请先选择候选人");
    return;
  }

  generating.value = true;
  try {
    await loadCandidateContext();
    const kit = await store.generateInterviewKit(
      selectedCandidateId.value,
      selectedJobId.value ?? undefined,
    );
    questionDrafts.value = kit.questions.map(fromQuestion);
    toast.success(`题库已生成，共 ${kit.questions.length} 题`);
  } finally {
    generating.value = false;
  }
}

function addQuestion() {
  questionDrafts.value.push(createDraft());
}

function removeQuestion(index: number) {
  questionDrafts.value.splice(index, 1);
}

async function saveKit() {
  if (!selectedCandidateId.value) {
    toast.warning("请先选择候选人");
    return;
  }

  const questions = questionDrafts.value
    .map((draft) => ({
      primary_question: draft.primaryQuestion.trim(),
      follow_ups: parseLineList(draft.followUpsText),
      scoring_points: parseLineList(draft.scoringPointsText),
      red_flags: parseLineList(draft.redFlagsText),
    }))
    .filter((item) => item.primary_question.length > 0);

  if (questions.length === 0) {
    toast.warning("请至少保留一条题目");
    return;
  }

  if (questions.some((item) => item.follow_ups.length === 0 || item.scoring_points.length === 0)) {
    toast.warning("每道题至少填写一条追问和评分要点");
    return;
  }

  savingKit.value = true;
  try {
    const saved = await store.saveInterviewKit({
      candidate_id: selectedCandidateId.value,
      job_id: selectedJobId.value ?? undefined,
      questions,
    });
    questionDrafts.value = saved.questions.map(fromQuestion);
    toast.success("题库已保存");
  } finally {
    savingKit.value = false;
  }
}

function buildStructuredFeedback() {
  return {
    summary: feedbackSummary.value.trim(),
    scores: {
      communication: Number(feedbackScores.communication),
      problem_solving: Number(feedbackScores.problem_solving),
      execution: Number(feedbackScores.execution),
      values_fit: Number(feedbackScores.values_fit),
    },
    red_flags: parseLineList(feedbackRedFlags.value),
  };
}

async function submitFeedback() {
  if (!selectedCandidateId.value) {
    toast.warning("请先选择候选人");
    return null;
  }
  if (!transcriptText.value.trim()) {
    toast.warning("请填写面试转写文本");
    return null;
  }
  if (!feedbackSummary.value.trim()) {
    toast.warning("请填写结构化评价摘要");
    return null;
  }

  submitting.value = true;
  try {
    const feedback = await store.submitInterviewFeedback({
      candidate_id: selectedCandidateId.value,
      job_id: selectedJobId.value ?? undefined,
      transcript_text: transcriptText.value,
      structured_feedback: buildStructuredFeedback(),
      recording_path: recordingPath.value.trim() || undefined,
    });
    toast.success("面后反馈已提交");
    return feedback;
  } finally {
    submitting.value = false;
  }
}

async function runEvaluation(feedbackId?: number) {
  if (!selectedCandidateId.value) {
    toast.warning("请先选择候选人");
    return;
  }

  evaluating.value = true;
  try {
    const result = await store.runInterviewEvaluation({
      candidate_id: selectedCandidateId.value,
      job_id: selectedJobId.value ?? undefined,
      feedback_id: feedbackId,
    });
    toast.success(`评估完成：${interviewRecommendationLabel(result.recommendation)}`);
  } finally {
    evaluating.value = false;
  }
}

async function submitAndEvaluate() {
  const feedback = await submitFeedback();
  if (!feedback) {
    return;
  }

  await runEvaluation(feedback.id);
}
</script>

<template>
  <section class="flex flex-col gap-4">
    <UiPanel title="面试题库与面后评估">
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
        <div class="flex items-end gap-2 flex-wrap">
          <UiButton variant="secondary" :disabled="!selectedCandidateId" @click="loadCandidateContext">刷新上下文</UiButton>
          <UiButton :disabled="!selectedCandidateId || generating" @click="generateKit">
            {{ generating ? "生成中..." : "生成题库" }}
          </UiButton>
        </div>
      </div>

      <div v-if="selectedCandidate" class="mt-3 grid grid-cols-2 gap-2.5 lt-lg:grid-cols-1">
        <UiInfoRow label="当前候选人" :value="`${selectedCandidate.name}（${selectedCandidate.years_of_experience}年）`" />
        <UiInfoRow label="阶段">
          <UiBadge :tone="stageTone(selectedCandidate.stage)">{{ formatStageLabel(selectedCandidate.stage) }}</UiBadge>
        </UiInfoRow>
        <UiInfoRow v-if="latestScreening" label="初筛结论">
          <UiBadge :tone="latestScreening.recommendation === 'PASS' ? 'success' : latestScreening.recommendation === 'REVIEW' ? 'warning' : 'danger'">
            {{ latestScreening.recommendation }} / 风险 {{ latestScreening.risk_level }}
          </UiBadge>
        </UiInfoRow>
      </div>
    </UiPanel>

    <UiPanel title="题库编辑">
      <div class="flex items-center gap-2 mb-2.5">
        <UiButton variant="secondary" @click="addQuestion">新增题目</UiButton>
        <UiButton :disabled="savingKit || !selectedCandidateId" @click="saveKit">
          {{ savingKit ? "保存中..." : "保存题库" }}
        </UiButton>
      </div>

      <UiTable>
        <thead>
          <tr>
            <UiTh>主问题</UiTh>
            <UiTh>追问链（每行一条）</UiTh>
            <UiTh>评分要点（每行一条）</UiTh>
            <UiTh>红旗信号（每行一条）</UiTh>
            <UiTh>操作</UiTh>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(question, index) in questionDrafts" :key="`${index}-${question.primaryQuestion}`">
            <UiTd>
              <textarea v-model="question.primaryQuestion" rows="4" placeholder="请输入主问题" />
            </UiTd>
            <UiTd>
              <textarea v-model="question.followUpsText" rows="4" placeholder="每行一个追问" />
            </UiTd>
            <UiTd>
              <textarea v-model="question.scoringPointsText" rows="4" placeholder="每行一个评分要点" />
            </UiTd>
            <UiTd>
              <textarea v-model="question.redFlagsText" rows="4" placeholder="每行一个红旗信号" />
            </UiTd>
            <UiTd>
              <UiButton variant="ghost" @click="removeQuestion(index)">删除</UiButton>
            </UiTd>
          </tr>
        </tbody>
      </UiTable>
      <p v-if="questionDrafts.length === 0" class="text-muted text-[0.85rem] mt-2">暂无题目，请先生成或新增。</p>
    </UiPanel>

    <UiPanel title="面后反馈提交">
      <div class="grid grid-cols-2 gap-2.5 lt-lg:grid-cols-1">
        <UiField label="面试转写文本（必填）">
          <textarea v-model="transcriptText" rows="8" placeholder="粘贴完整面试问答转写" />
        </UiField>
        <div class="flex flex-col gap-2.5">
          <UiField label="结构化评价摘要（必填）">
            <textarea v-model="feedbackSummary" rows="3" placeholder="简要总结候选人表现" />
          </UiField>
          <UiField label="红旗信号（可选）" help="每行一条">
            <textarea v-model="feedbackRedFlags" rows="3" placeholder="例如：回避关键指标" />
          </UiField>
          <UiField label="录音文件路径（可选）">
            <input v-model="recordingPath" placeholder="本地录音路径（仅存档）" />
          </UiField>
        </div>
      </div>

      <div class="grid grid-cols-4 gap-2.5 mt-2.5 lt-lg:grid-cols-2 lt-sm:grid-cols-1">
        <UiField label="沟通表达（1-5）">
          <input v-model.number="feedbackScores.communication" type="number" min="1" max="5" step="0.5" />
        </UiField>
        <UiField label="问题解决（1-5）">
          <input v-model.number="feedbackScores.problem_solving" type="number" min="1" max="5" step="0.5" />
        </UiField>
        <UiField label="执行推进（1-5）">
          <input v-model.number="feedbackScores.execution" type="number" min="1" max="5" step="0.5" />
        </UiField>
        <UiField label="价值观匹配（1-5）">
          <input v-model.number="feedbackScores.values_fit" type="number" min="1" max="5" step="0.5" />
        </UiField>
      </div>

      <div class="flex items-center gap-2.5 mt-3 flex-wrap">
        <UiButton variant="secondary" :disabled="submitting || !selectedCandidateId" @click="submitFeedback">
          {{ submitting ? "提交中..." : "仅提交反馈" }}
        </UiButton>
        <UiButton :disabled="submitting || evaluating || !selectedCandidateId" @click="submitAndEvaluate">
          {{ evaluating ? "评估中..." : "提交并运行评估" }}
        </UiButton>
        <UiButton
          variant="ghost"
          :disabled="evaluating || !selectedCandidateId"
          @click="runEvaluation(latestFeedback?.id)"
        >
          {{ evaluating ? "评估中..." : "使用最近反馈重跑评估" }}
        </UiButton>
      </div>
    </UiPanel>

    <UiPanel v-if="latestEvaluation" title="最新面后评估">
      <div class="flex items-center gap-2 mb-2">
        <UiBadge :tone="interviewRecommendationTone(latestEvaluation.recommendation)">
          {{ interviewRecommendationLabel(latestEvaluation.recommendation) }}
        </UiBadge>
        <span>综合分 {{ latestEvaluation.overall_score }}</span>
        <span class="text-muted">置信度 {{ latestEvaluation.confidence.toFixed(2) }}</span>
      </div>

      <UiInfoRow label="不确定性说明" :value="latestEvaluation.uncertainty" />

      <p class="m-0 mt-2 mb-1 font-600">证据引用</p>
      <ul class="mt-1 pl-4.5">
        <li v-for="item in latestEvaluation.evidence" :key="item" class="mb-1">{{ item }}</li>
      </ul>

      <p class="m-0 mt-2 mb-1 font-600">人工核验点</p>
      <ul class="mt-1 pl-4.5">
        <li v-for="item in latestEvaluation.verification_points" :key="item" class="mb-1">{{ item }}</li>
      </ul>
    </UiPanel>
  </section>
</template>
