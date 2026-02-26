<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import type { CandidateGender, CandidateRecord, CrawlMode, PipelineStage } from "@doss/shared";
import { useRouter } from "vue-router";
import { useRecruitingStore } from "../stores/recruiting";
import { formatStageLabel, nextStageOptions } from "../lib/pipeline";
import { stageTone } from "../lib/status";
import UiBadge from "../components/UiBadge.vue";
import UiButton from "../components/UiButton.vue";
import UiCheckbox from "../components/UiCheckbox.vue";
import UiField from "../components/UiField.vue";
import UiInfoRow from "../components/UiInfoRow.vue";
import UiPanel from "../components/UiPanel.vue";
import UiSelect, { type UiSelectOption } from "../components/UiSelect.vue";
import UiTable from "../components/UiTable.vue";
import UiTd from "../components/UiTd.vue";
import UiTh from "../components/UiTh.vue";
import { useToastStore } from "../stores/toast";

const store = useRecruitingStore();
const toast = useToastStore();
const router = useRouter();
const selectedCandidateId = ref<number | null>(null);
const searchKeyword = ref("");
const candidateDialogOpen = ref(false);
const candidateDialogSubmitting = ref(false);
const candidateDrawerOpen = ref(false);
const candidateDrawerSubmitting = ref(false);
const candidateDrawerLoading = ref(false);
const deleteConfirmCandidate = ref<CandidateRecord | null>(null);
const deletingCandidateId = ref<number | null>(null);

const candidateForm = reactive({
  name: "",
  current_company: "",
  job_id: "",
  score: null as number | null | "",
  age: null as number | null | "",
  gender: "" as CandidateGender | "",
  years_of_experience: 3,
  phone: "",
  email: "",
  tags: "",
  extra_description: "",
});
const candidateDialogResumeFile = ref<File | null>(null);
const candidateDialogResumeEnableOcr = ref(false);
const candidateDrawerForm = reactive({
  name: "",
  current_company: "",
  job_id: "",
  score: null as number | null | "",
  age: null as number | null | "",
  gender: "" as CandidateGender | "",
  years_of_experience: 3,
  phone: "",
  email: "",
  tags: "",
});

const resumeText = ref("");
const resumeSkills = ref("Vue3, TypeScript, SQL");
const resumeFile = ref<File | null>(null);
const resumeImportEnableOcr = ref(false);
const stageNote = ref("");
const sidecarResumeSource = ref<"boss" | "zhilian" | "wuba" | "lagou">("boss");
const sidecarResumeMode = ref<CrawlMode>("compliant");
const genderOptions: UiSelectOption[] = [
  { label: "未填写", value: "" },
  { label: "男", value: "male" },
  { label: "女", value: "female" },
  { label: "其他", value: "other" },
];
const sidecarResumeSourceOptions: UiSelectOption[] = [
  { label: "Boss", value: "boss" },
  { label: "智联", value: "zhilian" },
  { label: "58", value: "wuba" },
  { label: "拉勾", value: "lagou" },
];
const sidecarResumeModeOptions: UiSelectOption[] = [
  { label: "合规模式", value: "compliant" },
  { label: "高级模式", value: "advanced" },
];

const selectedCandidate = computed(() =>
  store.candidates.find((item) => item.id === selectedCandidateId.value) ?? null,
);

const candidateJobOptions = computed<UiSelectOption[]>(() =>
  store.jobs.map((job) => ({
    label: `${job.title} · ${job.company}`,
    value: String(job.id),
  })),
);

const candidateDrawerJobOptions = computed<UiSelectOption[]>(() => {
  const options = [...candidateJobOptions.value];
  const selected = selectedCandidate.value;
  if (!selected?.job_id) {
    return options;
  }

  const selectedValue = String(selected.job_id);
  const exists = options.some((item) => String(item.value) === selectedValue);
  if (exists) {
    return options;
  }

  options.unshift({
    label: `${selected.job_title || `职位 #${selected.job_id}`}（已删除）`,
    value: selectedValue,
  });
  return options;
});

const selectedAnalysis = computed(() => {
  if (!selectedCandidateId.value) {
    return [];
  }
  return store.analyses[selectedCandidateId.value] ?? [];
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

const candidateDialogTitle = computed(() => "新增候选人");

function formatGender(gender?: CandidateRecord["gender"]): string {
  if (gender === "male") {
    return "男";
  }
  if (gender === "female") {
    return "女";
  }
  if (gender === "other") {
    return "其他";
  }
  return "-";
}

function normalizeCandidateAge(value: number | null | ""): number | undefined {
  const age = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(age) || age < 0) {
    return undefined;
  }
  return Math.trunc(age);
}

function normalizeCandidateGender(value: CandidateGender | ""): CandidateGender | undefined {
  return value || undefined;
}

function normalizeCandidateScore(value: number | null | ""): number | undefined {
  const score = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(score) || score < 0) {
    return undefined;
  }
  return Math.max(0, Math.min(100, Math.round(score)));
}

function normalizeCandidateJobId(value: string | number | null | undefined): number | undefined {
  if (value === "" || value === null || value === undefined) {
    return undefined;
  }
  const parsed = typeof value === "number" ? value : Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    return undefined;
  }
  return parsed;
}

function formatCandidateJob(candidate: CandidateRecord): string {
  if (candidate.job_title && candidate.job_title.trim()) {
    return candidate.job_title;
  }
  if (candidate.job_id) {
    return `职位 #${candidate.job_id}`;
  }
  return "-";
}

function resetCandidateForm() {
  candidateForm.name = "";
  candidateForm.current_company = "";
  candidateForm.job_id = "";
  candidateForm.score = null;
  candidateForm.age = null;
  candidateForm.gender = "";
  candidateForm.years_of_experience = 3;
  candidateForm.phone = "";
  candidateForm.email = "";
  candidateForm.tags = "";
  candidateForm.extra_description = "";
  candidateDialogResumeFile.value = null;
  candidateDialogResumeEnableOcr.value = false;
}

function openCreateCandidateDialog() {
  resetCandidateForm();
  candidateDialogOpen.value = true;
}

function closeCandidateDialog() {
  candidateDialogOpen.value = false;
  resetCandidateForm();
}

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

function hydrateCandidateDrawerForm(candidate: CandidateRecord) {
  candidateDrawerForm.name = candidate.name;
  candidateDrawerForm.current_company = candidate.current_company ?? "";
  candidateDrawerForm.job_id = candidate.job_id ? String(candidate.job_id) : "";
  candidateDrawerForm.score = candidate.score ?? null;
  candidateDrawerForm.age = candidate.age ?? null;
  candidateDrawerForm.gender = candidate.gender ?? "";
  candidateDrawerForm.years_of_experience = candidate.years_of_experience;
  candidateDrawerForm.phone = "";
  candidateDrawerForm.email = "";
  candidateDrawerForm.tags = candidate.tags.join(", ");
}

function closeCandidateDrawer() {
  candidateDrawerOpen.value = false;
}

async function openCandidateDrawer(candidateId: number) {
  selectedCandidateId.value = candidateId;
  const candidate = store.candidates.find((item) => item.id === candidateId);
  if (candidate) {
    hydrateCandidateDrawerForm(candidate);
  }

  candidateDrawerOpen.value = true;
  candidateDrawerLoading.value = true;
  try {
    await store.loadCandidateContext(candidateId);
  } catch (error) {
    toast.warning(resolveErrorMessage(error, "候选人上下文加载失败"), 4200);
  } finally {
    candidateDrawerLoading.value = false;
  }
}

async function saveCandidateFromDrawer() {
  const candidate = selectedCandidate.value;
  if (!candidate || candidateDrawerSubmitting.value) {
    return;
  }
  if (!candidateDrawerForm.name.trim()) {
    toast.warning("请填写候选人姓名");
    return;
  }

  candidateDrawerSubmitting.value = true;
  try {
    const updated = await store.updateCandidate({
      candidate_id: candidate.id,
      name: candidateDrawerForm.name.trim(),
      current_company: candidateDrawerForm.current_company || undefined,
      job_id: normalizeCandidateJobId(candidateDrawerForm.job_id),
      score: normalizeCandidateScore(candidateDrawerForm.score),
      age: normalizeCandidateAge(candidateDrawerForm.age),
      gender: normalizeCandidateGender(candidateDrawerForm.gender),
      years_of_experience: Number(candidateDrawerForm.years_of_experience),
      phone: candidateDrawerForm.phone || undefined,
      email: candidateDrawerForm.email || undefined,
      tags: candidateDrawerForm.tags
        .split(",")
        .map((item) => item.trim())
        .filter(Boolean),
    });
    hydrateCandidateDrawerForm(updated);
    toast.success("候选人已更新");
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "候选人更新失败"));
  } finally {
    candidateDrawerSubmitting.value = false;
  }
}

function onCandidateDialogResumeFileChange(event: Event) {
  const input = event.target as HTMLInputElement;
  candidateDialogResumeFile.value = input.files?.[0] ?? null;
}

async function saveCandidateResumeFromDialog(candidateId: number) {
  const extraDescription = candidateForm.extra_description.trim();
  if (candidateDialogResumeFile.value) {
    const parsed = await store.importResumeFileAndAnalyze({
      candidateId,
      file: candidateDialogResumeFile.value,
      enableOcr: candidateDialogResumeEnableOcr.value,
    });

    if (extraDescription) {
      await store.saveResume({
        candidate_id: candidateId,
        raw_text: `${parsed.raw_text}\n\n额外描述：\n${extraDescription}`,
        parsed: {
          ...parsed.parsed,
          extraDescription,
        },
      });
    }

    const ocrUsed = parsed.metadata.ocrUsed === true ? "（已使用OCR）" : "";
    toast.success(`已导入 ${candidateDialogResumeFile.value.name}${ocrUsed}`, 4200);
    return;
  }

  if (extraDescription) {
    await store.saveResume({
      candidate_id: candidateId,
      raw_text: extraDescription,
      parsed: {
        skills: [],
        extraDescription,
      },
    });
  }
}

async function submitCandidate() {
  if (!candidateForm.name.trim() || candidateDialogSubmitting.value) {
    return;
  }

  const payload = {
    name: candidateForm.name,
    current_company: candidateForm.current_company || undefined,
    job_id: normalizeCandidateJobId(candidateForm.job_id),
    score: normalizeCandidateScore(candidateForm.score),
    age: normalizeCandidateAge(candidateForm.age),
    gender: normalizeCandidateGender(candidateForm.gender),
    years_of_experience: Number(candidateForm.years_of_experience),
    phone: candidateForm.phone || undefined,
    email: candidateForm.email || undefined,
    tags: candidateForm.tags
      .split(",")
      .map((item) => item.trim())
      .filter(Boolean),
  };
  candidateDialogSubmitting.value = true;

  try {
    const candidate = await store.addCandidate(payload);
    await saveCandidateResumeFromDialog(candidate.id);
    closeCandidateDialog();
    toast.success("候选人创建成功");
    await openCandidateDrawer(candidate.id);
  } catch (error) {
    toast.danger(error instanceof Error ? error.message : "候选人创建失败");
  } finally {
    candidateDialogSubmitting.value = false;
  }
}

async function saveResume() {
  if (!selectedCandidateId.value || !resumeText.value.trim()) {
    return;
  }

  await store.saveResume({
    candidate_id: selectedCandidateId.value,
    raw_text: resumeText.value,
    parsed: {
      skills: resumeSkills.value
        .split(",")
        .map((item) => item.trim())
        .filter(Boolean),
      expectedSalaryK: 45,
    },
  });
}

function onResumeFileChange(event: Event) {
  const input = event.target as HTMLInputElement;
  resumeFile.value = input.files?.[0] ?? null;
}

async function importResumeFileAndAnalyze() {
  if (!selectedCandidateId.value || !resumeFile.value) {
    return;
  }

  const parsed = await store.importResumeFileAndAnalyze({
    candidateId: selectedCandidateId.value,
    file: resumeFile.value,
    enableOcr: resumeImportEnableOcr.value,
  });

  resumeText.value = parsed.raw_text;
  const parsedSkills = parsed.parsed.skills;
  if (Array.isArray(parsedSkills)) {
    resumeSkills.value = parsedSkills
      .filter((item): item is string => typeof item === "string")
      .join(", ");
  }

  const ocrUsed = parsed.metadata.ocrUsed === true ? "（已使用OCR）" : "";
  toast.success(`已导入 ${resumeFile.value.name} 并触发分析${ocrUsed}`, 4800);
}

async function runAnalysis() {
  if (!selectedCandidateId.value) {
    return;
  }

  await store.analyzeCandidate(selectedCandidateId.value);
}

async function runScreening() {
  if (!selectedCandidateId.value) {
    return;
  }

  await store.runScreening(selectedCandidateId.value);
  toast.success("初筛结果已更新");
}

async function moveStage(stage: PipelineStage) {
  if (!selectedCandidateId.value) {
    return;
  }

  await store.moveStage({
    candidate_id: selectedCandidateId.value,
    to_stage: stage,
    note: stageNote.value || undefined,
  });
  stageNote.value = "";
}

function openInterviewWorkspace() {
  if (!selectedCandidateId.value) {
    return;
  }
  router.push({
    path: "/interview",
    query: {
      candidateId: String(selectedCandidateId.value),
    },
  });
}

function openDecisionWorkspace() {
  if (!selectedCandidateId.value) {
    return;
  }
  router.push({
    path: "/decision",
    query: {
      candidateId: String(selectedCandidateId.value),
    },
  });
}

async function doSearch() {
  const result = await store.search(searchKeyword.value);
  if (!result.ok) {
    toast.warning(`搜索失败，已清空结果：${result.error ?? "unknown_error"}`, 4200);
  }
}

function askDeleteCandidate(candidate: CandidateRecord) {
  deleteConfirmCandidate.value = candidate;
}

function cancelDeleteCandidate() {
  if (deletingCandidateId.value !== null) {
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
    if (selectedCandidateId.value === candidate.id) {
      selectedCandidateId.value = null;
      candidateDrawerOpen.value = false;
    }
    deleteConfirmCandidate.value = null;
    toast.success("候选人已删除");
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "删除候选人失败"));
  } finally {
    deletingCandidateId.value = null;
  }
}

async function toggleCandidateQualification(candidate: CandidateRecord) {
  const qualified = candidate.stage === "REJECTED";
  await store.setCandidateQualification({
    candidate_id: candidate.id,
    qualified,
    note: qualified ? "手动启用资格" : "手动取消资格",
  });
  if (selectedCandidateId.value === candidate.id) {
    await openCandidateDrawer(candidate.id);
  }
  toast.success(qualified ? "候选人资格已启用" : "候选人资格已取消");
}

async function crawlResumeFromSidecar() {
  if (!selectedCandidate.value?.external_id) {
    toast.warning("当前候选人无外部ID，无法从招聘站抓取简历。");
    return;
  }

  const source = selectedCandidate.value.source;
  if (source === "boss" || source === "zhilian" || source === "wuba" || source === "lagou") {
    sidecarResumeSource.value = source;
  }

  const response = await store.runSidecarResumeCrawl({
    source: sidecarResumeSource.value,
    mode: sidecarResumeMode.value,
    localCandidateId: selectedCandidate.value.id,
    externalCandidateId: selectedCandidate.value.external_id,
  });
  toast.success(`简历已抓取并写入，任务状态: ${response.result.status}`, 4200);
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

function screeningLabel(recommendation: "PASS" | "REVIEW" | "REJECT") {
  if (recommendation === "PASS") {
    return "通过初筛";
  }
  if (recommendation === "REVIEW") {
    return "建议复核";
  }
  return "不通过";
}
</script>

<template>
  <section class="flex flex-col gap-4">
    <div class="flex flex-col gap-4">
      <div class="flex items-center justify-end">
        <UiButton @click="openCreateCandidateDialog">新增候选人</UiButton>
      </div>

      <UiPanel>
        <template #header>
          <div class="flex items-center gap-2.5 mb-2.5 flex-wrap">
            <h3 class="text-lg font-700">候选人列表</h3>
            <input v-model="searchKeyword" class="min-w-60 flex-1" placeholder="本地搜索关键词" @keyup.enter="doSearch" />
            <UiButton variant="secondary" @click="doSearch">搜索</UiButton>
          </div>
        </template>
        <UiTable>
          <thead>
            <tr>
              <UiTh>姓名</UiTh>
              <UiTh>当前公司</UiTh>
              <UiTh>职位</UiTh>
              <UiTh>评分</UiTh>
              <UiTh>年龄</UiTh>
              <UiTh>性别</UiTh>
              <UiTh>年限</UiTh>
              <UiTh>阶段</UiTh>
              <UiTh>标签</UiTh>
              <UiTh>操作</UiTh>
            </tr>
          </thead>
          <tbody>
            <tr v-for="candidate in store.candidates" :key="candidate.id">
              <UiTd>{{ candidate.name }}</UiTd>
              <UiTd>{{ candidate.current_company || "-" }}</UiTd>
              <UiTd>{{ formatCandidateJob(candidate) }}</UiTd>
              <UiTd>{{ candidate.score ?? "-" }}</UiTd>
              <UiTd>{{ candidate.age ?? "-" }}</UiTd>
              <UiTd>{{ formatGender(candidate.gender) }}</UiTd>
              <UiTd>{{ candidate.years_of_experience }}</UiTd>
              <UiTd>
                <UiBadge :tone="stageTone(candidate.stage)">{{ formatStageLabel(candidate.stage) }}</UiBadge>
              </UiTd>
              <UiTd>{{ candidate.tags.join(" / ") || "-" }}</UiTd>
              <UiTd>
                <div class="flex items-center gap-1.5 flex-wrap">
                  <UiButton variant="ghost" @click="openCandidateDrawer(candidate.id)">查看/编辑</UiButton>
                  <UiButton variant="ghost" @click="toggleCandidateQualification(candidate)">
                    {{ candidate.stage === "REJECTED" ? "启用资格" : "取消资格" }}
                  </UiButton>
                  <UiButton
                    variant="ghost"
                    :disabled="deletingCandidateId === candidate.id"
                    @click="askDeleteCandidate(candidate)"
                  >
                    {{ deletingCandidateId === candidate.id ? "删除中..." : "删除" }}
                  </UiButton>
                </div>
              </UiTd>
            </tr>
          </tbody>
        </UiTable>

        <div v-if="store.searchResults.length" class="mt-3">
          <h4 class="text-base font-600">搜索结果</h4>
          <ul class="mt-2 pl-4.5">
            <li
              v-for="result in store.searchResults"
              :key="`${result.candidate_id}-${result.snippet}`"
              class="mb-1.5 flex items-start gap-1.5 flex-wrap"
            >
              <span>#{{ result.candidate_id }} {{ result.name }}</span>
              <span>-</span>
              <UiBadge :tone="stageTone(result.stage)">{{ formatStageLabel(result.stage) }}</UiBadge>
              <span class="text-muted"> {{ result.snippet }}</span>
            </li>
          </ul>
        </div>
      </UiPanel>
    </div>
  </section>

  <Teleport to="body">
    <div
      v-if="candidateDialogOpen"
      class="fixed inset-0 z-50 bg-black/42 backdrop-blur-[2px] px-4 py-6 flex items-center justify-center"
      @click.self="closeCandidateDialog"
    >
      <UiPanel class="w-full max-w-3xl max-h-[86vh] overflow-y-auto">
        <template #header>
          <div class="flex items-center justify-between gap-2 mb-2.5">
            <h3 class="text-lg font-700">{{ candidateDialogTitle }}</h3>
            <UiButton variant="ghost" @click="closeCandidateDialog">关闭</UiButton>
          </div>
        </template>

        <div class="grid grid-cols-2 gap-2.5 mb-2.5 lt-lg:grid-cols-1">
          <UiField label="姓名">
            <input v-model="candidateForm.name" placeholder="候选人姓名" />
          </UiField>
          <UiField label="当前公司">
            <input v-model="candidateForm.current_company" placeholder="当前就职公司" />
          </UiField>
          <UiField label="关联职位">
            <UiSelect
              v-model="candidateForm.job_id"
              :options="candidateJobOptions"
              placeholder="不关联职位"
            />
          </UiField>
          <UiField label="评分">
            <input v-model.number="candidateForm.score" type="number" min="0" max="100" step="1" placeholder="评分（0-100）" />
          </UiField>
          <UiField label="年龄">
            <input v-model.number="candidateForm.age" type="number" min="0" step="1" placeholder="年龄（可选）" />
          </UiField>
          <UiField label="性别">
            <UiSelect v-model="candidateForm.gender" :options="genderOptions" />
          </UiField>
          <UiField label="年限">
            <input
              v-model.number="candidateForm.years_of_experience"
              type="number"
              min="0"
              step="0.5"
              placeholder="工作年限"
            />
          </UiField>
          <UiField label="标签">
            <input v-model="candidateForm.tags" placeholder="标签，逗号分隔" />
          </UiField>
          <UiField label="手机号">
            <input v-model="candidateForm.phone" placeholder="手机号（编辑时留空表示不更新）" />
          </UiField>
          <UiField label="邮箱">
            <input v-model="candidateForm.email" placeholder="邮箱（编辑时留空表示不更新）" />
          </UiField>
        </div>

        <UiField label="额外描述" class="mb-2.5">
          <textarea v-model="candidateForm.extra_description" rows="4" placeholder="补充项目背景、期望岗位、候选人亮点等（可选）" />
        </UiField>

        <div class="grid grid-cols-2 gap-2.5 mb-3 lt-lg:grid-cols-1">
          <UiField label="上传简历（可选）">
            <input
              type="file"
              accept=".pdf,.docx,.txt,.md,.png,.jpg,.jpeg,.bmp,.tif,.tiff"
              @change="onCandidateDialogResumeFileChange"
            />
          </UiField>
          <div class="flex items-end">
            <UiCheckbox v-model="candidateDialogResumeEnableOcr" label="导入时启用 OCR（可选）" />
          </div>
        </div>

        <div class="flex items-center justify-end gap-2">
          <UiButton variant="secondary" :disabled="candidateDialogSubmitting" @click="closeCandidateDialog">
            取消
          </UiButton>
          <UiButton :disabled="candidateDialogSubmitting" @click="submitCandidate">
            {{ candidateDialogSubmitting ? "处理中..." : "创建候选人" }}
          </UiButton>
        </div>
      </UiPanel>
    </div>
  </Teleport>

  <Teleport to="body">
    <div
      v-if="candidateDrawerOpen && selectedCandidate"
      class="fixed inset-0 z-50 pointer-events-none"
    >
      <div class="absolute inset-0 bg-black/26 pointer-events-auto" @click="closeCandidateDrawer" />
      <aside class="absolute right-0 top-0 h-full w-full max-w-2xl bg-bg border-l border-line p-4 overflow-y-auto pointer-events-auto">
        <div class="flex items-center justify-between gap-2 mb-3">
          <h3 class="text-lg font-700">详情</h3>
          <UiButton variant="ghost" @click="closeCandidateDrawer">关闭</UiButton>
        </div>

        <UiPanel :title="`候选人详情: ${selectedCandidate.name}`">
          <div class="grid grid-cols-2 gap-2.5 mb-2.5 lt-lg:grid-cols-1">
            <UiField label="姓名">
              <input v-model="candidateDrawerForm.name" placeholder="候选人姓名" />
            </UiField>
            <UiField label="当前公司">
              <input v-model="candidateDrawerForm.current_company" placeholder="当前就职公司" />
            </UiField>
            <UiField label="关联职位">
              <UiSelect
                v-model="candidateDrawerForm.job_id"
                :options="candidateDrawerJobOptions"
                placeholder="不关联职位"
              />
            </UiField>
            <UiField label="评分">
              <input
                v-model.number="candidateDrawerForm.score"
                type="number"
                min="0"
                max="100"
                step="1"
                placeholder="评分（0-100）"
              />
            </UiField>
            <UiField label="年龄">
              <input v-model.number="candidateDrawerForm.age" type="number" min="0" step="1" placeholder="年龄（可选）" />
            </UiField>
            <UiField label="性别">
              <UiSelect v-model="candidateDrawerForm.gender" :options="genderOptions" />
            </UiField>
            <UiField label="年限">
              <input
                v-model.number="candidateDrawerForm.years_of_experience"
                type="number"
                min="0"
                step="0.5"
                placeholder="工作年限"
              />
            </UiField>
            <UiField label="标签">
              <input v-model="candidateDrawerForm.tags" placeholder="标签，逗号分隔" />
            </UiField>
            <UiField label="手机号">
              <input v-model="candidateDrawerForm.phone" placeholder="手机号（留空表示不更新）" />
            </UiField>
            <UiField label="邮箱">
              <input v-model="candidateDrawerForm.email" placeholder="邮箱（留空表示不更新）" />
            </UiField>
          </div>
          <div class="flex items-center justify-end gap-2 mb-2.5">
            <UiButton :disabled="candidateDrawerSubmitting" @click="saveCandidateFromDrawer">
              {{ candidateDrawerSubmitting ? "保存中..." : "保存候选人" }}
            </UiButton>
          </div>
          <div class="flex flex-col gap-1.5 mb-2">
            <UiInfoRow label="阶段">
              <UiBadge :tone="stageTone(selectedCandidate.stage)">{{ formatStageLabel(selectedCandidate.stage) }}</UiBadge>
            </UiInfoRow>
            <UiInfoRow label="电话" :value="selectedCandidate.phone_masked || '-'" />
            <UiInfoRow label="邮箱" :value="selectedCandidate.email_masked || '-'" />
          </div>

          <UiField label="阶段变更备注">
            <input v-model="stageNote" placeholder="记录本次阶段调整原因（可选）" />
          </UiField>

          <div class="flex flex-wrap gap-2 mt-2">
            <UiButton variant="secondary" @click="openInterviewWorkspace">进入面试页</UiButton>
            <UiButton variant="secondary" @click="openDecisionWorkspace">进入决策页</UiButton>
            <UiButton
              v-for="stage in nextStageOptions(selectedCandidate.stage)"
              :key="stage"
              variant="secondary"
              @click="moveStage(stage)"
            >
              迁移到 {{ formatStageLabel(stage) }}
            </UiButton>
          </div>
        </UiPanel>

        <UiPanel class="mt-3" title="简历录入">
          <UiField label="简历正文">
            <textarea v-model="resumeText" rows="6" placeholder="粘贴简历文本" />
          </UiField>
          <UiField label="技能列表" class="mt-2.5">
            <input v-model="resumeSkills" placeholder="技能列表，逗号分隔" />
          </UiField>
          <div class="grid grid-cols-2 gap-2.5 mb-2.5 mt-2.5 lt-lg:grid-cols-1">
            <UiField label="本地文件导入">
              <input type="file" accept=".pdf,.docx,.txt,.md,.png,.jpg,.jpeg,.bmp,.tif,.tiff" @change="onResumeFileChange" />
            </UiField>
            <div class="flex items-end">
              <UiCheckbox v-model="resumeImportEnableOcr" label="导入时启用 OCR（可选）" />
            </div>
          </div>
          <div class="grid grid-cols-2 gap-2.5 mb-2.5 lt-lg:grid-cols-1">
            <UiField label="简历来源平台">
              <UiSelect v-model="sidecarResumeSource" :options="sidecarResumeSourceOptions" />
            </UiField>
            <UiField label="抓取模式">
              <UiSelect v-model="sidecarResumeMode" :options="sidecarResumeModeOptions" />
            </UiField>
          </div>
          <div class="flex items-center gap-2.5 mb-2.5 flex-wrap">
            <UiButton @click="saveResume">保存简历</UiButton>
            <UiButton
              variant="secondary"
              :disabled="!resumeFile || !selectedCandidateId"
              @click="importResumeFileAndAnalyze"
            >
              上传PDF/DOCX并自动分析
            </UiButton>
            <UiButton
              variant="secondary"
              :disabled="!selectedCandidate.external_id"
              @click="crawlResumeFromSidecar"
            >
              Sidecar抓取简历并入库
            </UiButton>
            <UiButton variant="secondary" @click="runScreening">运行初筛</UiButton>
            <UiButton @click="runAnalysis">运行AI分析</UiButton>
          </div>
        </UiPanel>

        <UiPanel v-if="selectedScreening.length" class="mt-3" title="最新初筛">
          <div class="flex items-center gap-2 mb-2">
            <UiBadge :tone="screeningTone(selectedScreening[0].recommendation)">
              {{ screeningLabel(selectedScreening[0].recommendation) }}
            </UiBadge>
            <UiBadge :tone="selectedScreening[0].risk_level === 'HIGH' ? 'danger' : selectedScreening[0].risk_level === 'MEDIUM' ? 'warning' : 'info'">
              风险 {{ selectedScreening[0].risk_level }}
            </UiBadge>
          </div>
          <div class="grid grid-cols-2 gap-2 mb-2.5">
            <UiInfoRow label="T0" :value="selectedScreening[0].t0_score" />
            <UiInfoRow label="T1" :value="selectedScreening[0].t1_score" />
            <UiInfoRow label="精筛" :value="selectedScreening[0].fine_score" />
            <UiInfoRow label="综合分" :value="selectedScreening[0].overall_score" />
          </div>
          <p class="m-0 mb-1 font-600">证据</p>
          <ul class="mt-1 pl-4.5">
            <li v-for="item in selectedScreening[0].evidence" :key="item" class="mb-1">{{ item }}</li>
          </ul>
          <p class="m-0 mt-2 mb-1 font-600">核验建议</p>
          <ul class="mt-1 pl-4.5">
            <li v-for="item in selectedScreening[0].verification_points" :key="item" class="mb-1">{{ item }}</li>
          </ul>
        </UiPanel>

        <UiPanel v-if="selectedAnalysis.length" class="mt-3" title="最新分析">
          <p class="m-0 mb-2">总分: {{ selectedAnalysis[0].overallScore }}</p>
          <ul class="mt-2 pl-4.5">
            <li v-for="item in selectedAnalysis[0].dimensionScores" :key="item.key" class="mb-1.5">
              {{ item.key }}: {{ item.score }} ({{ item.reason }})
            </li>
          </ul>
          <p v-if="selectedAnalysis[0].risks.length" class="m-0 mt-2">风险: {{ selectedAnalysis[0].risks.join("；") }}</p>
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
              <p v-if="item.note" class="m-0 w-full text-[0.82rem] text-muted">
                备注: {{ item.note }}
              </p>
            </li>
          </ul>
        </UiPanel>

        <p v-if="candidateDrawerLoading" class="m-0 mt-3 text-sm text-muted">正在刷新候选人上下文...</p>
      </aside>
    </div>
  </Teleport>

  <Teleport to="body">
    <div
      v-if="deleteConfirmCandidate"
      class="fixed inset-0 z-[85] flex items-center justify-center bg-black/35 p-4"
      @click.self="cancelDeleteCandidate()"
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
              @click="cancelDeleteCandidate()"
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
