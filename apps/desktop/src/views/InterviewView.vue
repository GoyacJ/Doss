<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import type { CandidateRecord } from "@doss/shared";
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
import {
  interviewRecommendationLabel,
  interviewRecommendationTone,
  stageTone,
} from "../lib/status";
import { listInterviewCandidatesPage, saveInterviewRecording } from "../services/backend";
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
const router = useRouter();

const loading = ref(false);
const page = ref(1);
const pageSize = ref(10);
const total = ref(0);
const rows = ref<CandidateRecord[]>([]);

const filters = reactive({
  jobId: 0,
  nameLike: "",
});

const drawerOpen = ref(false);
const drawerLoading = ref(false);
const selectedCandidateId = ref<number | null>(null);
const selectedJobId = ref<number | null>(null);

const generating = ref(false);
const savingKit = ref(false);
const submitting = ref(false);
const evaluating = ref(false);
const actingCandidateId = ref<number | null>(null);

const questionDrafts = ref<InterviewQuestionDraft[]>([]);
const transcriptText = ref("");
const feedbackSummary = ref("");
const feedbackRedFlags = ref("");
const recordingPath = ref("");
const recordingFile = ref<File | null>(null);

const feedbackScores = reactive({
  communication: 3,
  problem_solving: 3,
  execution: 3,
  values_fit: 3,
});

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

const latestFeedback = computed(() => {
  if (!selectedCandidateId.value) {
    return null;
  }
  return store.interviewFeedback[selectedCandidateId.value]?.[0] ?? null;
});

const hasPrevPage = computed(() => page.value > 1);
const hasNextPage = computed(() => page.value * pageSize.value < total.value);
const totalPages = computed(() => Math.max(1, Math.ceil(total.value / pageSize.value)));

const jobOptions = computed(() => [
  { value: 0, label: "全部职位" },
  ...store.jobs.map((job) => ({ value: job.id, label: `${job.title} · ${job.company}` })),
]);

function resolveErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  if (typeof error === "string" && error.trim()) {
    return error;
  }
  return fallback;
}

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

async function loadRows() {
  loading.value = true;
  try {
    const data = await listInterviewCandidatesPage({
      page: page.value,
      page_size: pageSize.value,
      job_id: filters.jobId > 0 ? filters.jobId : undefined,
      name_like: filters.nameLike.trim() || undefined,
    });
    rows.value = data.items;
    total.value = data.total;
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "待面试列表加载失败"));
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
  selectedJobId.value = candidate.job_id ?? null;
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

async function removeInterview(candidate: CandidateRecord) {
  if (actingCandidateId.value) {
    return;
  }
  actingCandidateId.value = candidate.id;
  try {
    await store.moveStage({
      candidate_id: candidate.id,
      to_stage: "HOLD",
      note: "removed_from_interview",
      job_id: candidate.job_id ?? undefined,
    });
    await loadRows();
    toast.success("已移除面试");
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "移除面试失败"));
  } finally {
    actingCandidateId.value = null;
  }
}

async function markInterviewed(candidate: CandidateRecord) {
  if (actingCandidateId.value) {
    return;
  }
  actingCandidateId.value = candidate.id;
  try {
    await store.moveStage({
      candidate_id: candidate.id,
      to_stage: "HOLD",
      note: "interview_completed",
      job_id: candidate.job_id ?? undefined,
    });
    await loadRows();
    router.push({
      path: "/decision",
      query: {
        candidateId: String(candidate.id),
      },
    });
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "已面试动作失败"));
  } finally {
    actingCandidateId.value = null;
  }
}

async function generateKit(candidate: CandidateRecord) {
  if (generating.value) {
    return;
  }
  generating.value = true;
  try {
    await openDrawer(candidate);
    const kit = await store.generateInterviewKit(candidate.id, candidate.job_id ?? undefined);
    questionDrafts.value = kit.questions.map(fromQuestion);
    toast.success(`题库已生成，共 ${kit.questions.length} 题`);
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "生成题库失败"));
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
  if (!selectedCandidateId.value || savingKit.value) {
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

  savingKit.value = true;
  try {
    const saved = await store.saveInterviewKit({
      candidate_id: selectedCandidateId.value,
      job_id: selectedJobId.value ?? undefined,
      questions,
    });
    questionDrafts.value = saved.questions.map(fromQuestion);
    toast.success("题库已保存");
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "保存题库失败"));
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

async function fileToBase64(file: File): Promise<string> {
  const buffer = await file.arrayBuffer();
  const bytes = new Uint8Array(buffer);
  let binary = "";
  const chunkSize = 0x8000;
  for (let index = 0; index < bytes.length; index += chunkSize) {
    const chunk = bytes.subarray(index, index + chunkSize);
    binary += String.fromCharCode(...chunk);
  }
  return btoa(binary);
}

function onRecordingFileChange(event: Event) {
  const input = event.target as HTMLInputElement;
  recordingFile.value = input.files?.[0] ?? null;
}

async function resolveRecordingPath(): Promise<string | undefined> {
  const manual = recordingPath.value.trim();
  if (manual) {
    return manual;
  }
  if (!recordingFile.value) {
    return undefined;
  }

  const contentBase64 = await fileToBase64(recordingFile.value);
  const saved = await saveInterviewRecording({
    file_name: recordingFile.value.name,
    content_base64: contentBase64,
  });
  return saved.recording_path;
}

async function submitFeedback() {
  if (!selectedCandidateId.value || submitting.value) {
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
      recording_path: await resolveRecordingPath(),
    });
    toast.success("面后反馈已提交");
    return feedback;
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "提交面后反馈失败"));
    return null;
  } finally {
    submitting.value = false;
  }
}

async function runEvaluation(feedbackId?: number) {
  if (!selectedCandidateId.value || evaluating.value) {
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
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "运行面后评估失败"));
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
    <UiPanel title="待面试列表">
      <div class="grid grid-cols-4 gap-2.5 mb-3 lt-lg:grid-cols-2 lt-sm:grid-cols-1">
        <UiField label="职位筛选">
          <UiSelect v-model="filters.jobId" :options="jobOptions" value-type="number" />
        </UiField>
        <UiField label="姓名关键词">
          <input v-model="filters.nameLike" placeholder="输入姓名关键词" @keyup.enter="applyFilters" />
        </UiField>
        <div class="flex items-end gap-2 col-span-2 lt-sm:col-span-1">
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
                <UiButton
                  variant="ghost"
                  :disabled="actingCandidateId === candidate.id"
                  @click="removeInterview(candidate)"
                >
                  移除面试
                </UiButton>
                <UiButton
                  variant="secondary"
                  :disabled="generating || actingCandidateId === candidate.id"
                  @click="generateKit(candidate)"
                >
                  生成面试题
                </UiButton>
                <UiButton
                  :disabled="actingCandidateId === candidate.id"
                  @click="markInterviewed(candidate)"
                >
                  已面试
                </UiButton>
              </div>
            </UiTd>
          </tr>
          <tr v-if="!loading && rows.length === 0">
            <UiTd colspan="4" class="text-center text-muted py-6">暂无待面试候选人</UiTd>
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
      <aside class="absolute right-0 top-0 h-full w-full max-w-3xl bg-bg border-l border-line p-4 overflow-y-auto pointer-events-auto">
        <div class="flex items-center justify-between gap-2 mb-3">
          <h3 class="text-lg font-700">面试抽屉</h3>
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

        <UiPanel class="mt-3" title="题库编辑">
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
                <UiTh>追问链</UiTh>
                <UiTh>评分要点</UiTh>
                <UiTh>红旗信号</UiTh>
                <UiTh>操作</UiTh>
              </tr>
            </thead>
            <tbody>
              <tr v-for="(question, index) in questionDrafts" :key="`${index}-${question.primaryQuestion}`">
                <UiTd><textarea v-model="question.primaryQuestion" rows="4" placeholder="请输入主问题" /></UiTd>
                <UiTd><textarea v-model="question.followUpsText" rows="4" placeholder="每行一个追问" /></UiTd>
                <UiTd><textarea v-model="question.scoringPointsText" rows="4" placeholder="每行一个评分要点" /></UiTd>
                <UiTd><textarea v-model="question.redFlagsText" rows="4" placeholder="每行一个红旗信号" /></UiTd>
                <UiTd><UiButton variant="ghost" @click="removeQuestion(index)">删除</UiButton></UiTd>
              </tr>
            </tbody>
          </UiTable>
          <p v-if="questionDrafts.length === 0" class="text-muted text-[0.85rem] mt-2">暂无题目，请先生成或新增。</p>
        </UiPanel>

        <UiPanel class="mt-3" title="面后反馈提交">
          <div class="grid grid-cols-2 gap-2.5 lt-lg:grid-cols-1">
            <UiField label="面试转写文本（必填）">
              <textarea v-model="transcriptText" rows="8" placeholder="粘贴完整面试问答转写" />
            </UiField>
            <div class="flex flex-col gap-2.5">
              <UiField label="结构化评价摘要（必填)">
                <textarea v-model="feedbackSummary" rows="3" placeholder="简要总结候选人表现" />
              </UiField>
              <UiField label="红旗信号（可选）" help="每行一条">
                <textarea v-model="feedbackRedFlags" rows="3" placeholder="例如：回避关键指标" />
              </UiField>
              <UiField label="录音文件路径（可选）">
                <input v-model="recordingPath" placeholder="已有录音路径可直接填写" />
              </UiField>
              <UiField label="录音文件上传（可选）" help="上传后自动保存到 app data 目录">
                <input type="file" accept=".wav,.mp3,.m4a,.aac,.ogg,.webm" @change="onRecordingFileChange" />
              </UiField>
            </div>
          </div>

          <div class="grid grid-cols-4 gap-2.5 mt-2.5 lt-lg:grid-cols-2 lt-sm:grid-cols-1">
            <UiField label="沟通表达（1-5)"><input v-model.number="feedbackScores.communication" type="number" min="1" max="5" step="0.5" /></UiField>
            <UiField label="问题解决（1-5)"><input v-model.number="feedbackScores.problem_solving" type="number" min="1" max="5" step="0.5" /></UiField>
            <UiField label="执行推进（1-5)"><input v-model.number="feedbackScores.execution" type="number" min="1" max="5" step="0.5" /></UiField>
            <UiField label="价值观匹配（1-5)"><input v-model.number="feedbackScores.values_fit" type="number" min="1" max="5" step="0.5" /></UiField>
          </div>

          <div class="flex items-center gap-2.5 mt-3 flex-wrap">
            <UiButton variant="secondary" :disabled="submitting || !selectedCandidateId" @click="submitFeedback">
              {{ submitting ? "提交中..." : "仅提交反馈" }}
            </UiButton>
            <UiButton :disabled="submitting || evaluating || !selectedCandidateId" @click="submitAndEvaluate">
              {{ evaluating ? "评估中..." : "提交并运行评估" }}
            </UiButton>
            <UiButton variant="ghost" :disabled="evaluating || !selectedCandidateId" @click="runEvaluation(latestFeedback?.id)">
              {{ evaluating ? "评估中..." : "使用最近反馈重跑评估" }}
            </UiButton>
          </div>
        </UiPanel>

        <UiPanel v-if="latestEvaluation" class="mt-3" title="最新面后评估">
          <div class="flex items-center gap-2 mb-2 flex-wrap">
            <UiBadge :tone="interviewRecommendationTone(latestEvaluation.recommendation)">
              {{ interviewRecommendationLabel(latestEvaluation.recommendation) }}
            </UiBadge>
            <span>综合分 {{ latestEvaluation.overall_score }}</span>
            <span class="text-muted">置信度 {{ latestEvaluation.confidence.toFixed(2) }}</span>
          </div>
          <UiInfoRow label="不确定性说明" :value="latestEvaluation.uncertainty" />
        </UiPanel>

        <p v-if="drawerLoading" class="m-0 mt-3 text-sm text-muted">正在加载候选人上下文...</p>
      </aside>
    </div>
  </Teleport>
</template>
