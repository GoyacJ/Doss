<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import type { CrawlMode, PipelineStage } from "@doss/shared";
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
import UiTable from "../components/UiTable.vue";
import UiTd from "../components/UiTd.vue";
import UiTh from "../components/UiTh.vue";
import { useToastStore } from "../stores/toast";

const store = useRecruitingStore();
const toast = useToastStore();
const router = useRouter();
const selectedCandidateId = ref<number | null>(null);
const searchKeyword = ref("");

const candidateForm = reactive({
  name: "",
  current_company: "",
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

const selectedCandidate = computed(() =>
  store.candidates.find((item) => item.id === selectedCandidateId.value) ?? null,
);

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

async function submitCandidate() {
  if (!candidateForm.name.trim()) {
    return;
  }

  await store.addCandidate({
    name: candidateForm.name,
    current_company: candidateForm.current_company || undefined,
    years_of_experience: Number(candidateForm.years_of_experience),
    phone: candidateForm.phone || undefined,
    email: candidateForm.email || undefined,
    tags: candidateForm.tags
      .split(",")
      .map((item) => item.trim())
      .filter(Boolean),
  });

  candidateForm.name = "";
  candidateForm.current_company = "";
  candidateForm.years_of_experience = 3;
  candidateForm.phone = "";
  candidateForm.email = "";
  candidateForm.tags = "";
}

async function selectCandidate(candidateId: number) {
  selectedCandidateId.value = candidateId;
  await store.loadCandidateContext(candidateId);
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
  <section :class="selectedCandidate ? 'grid grid-cols-[1.1fr_1fr] gap-4 lt-lg:grid-cols-1' : 'flex flex-col gap-4'">
    <div class="flex flex-col gap-4">
      <UiPanel title="新增候选人">
        <div class="grid grid-cols-2 gap-2.5 mb-2.5 lt-lg:grid-cols-1">
          <UiField label="姓名">
            <input v-model="candidateForm.name" placeholder="候选人姓名" />
          </UiField>
          <UiField label="当前公司">
            <input v-model="candidateForm.current_company" placeholder="当前就职公司" />
          </UiField>
          <UiField label="年限">
            <input v-model.number="candidateForm.years_of_experience" type="number" min="0" step="0.5" placeholder="工作年限" />
          </UiField>
          <UiField label="手机号">
            <input v-model="candidateForm.phone" placeholder="手机号" />
          </UiField>
          <UiField label="邮箱">
            <input v-model="candidateForm.email" placeholder="邮箱地址" />
          </UiField>
          <UiField label="标签">
            <input v-model="candidateForm.tags" placeholder="标签，逗号分隔" />
          </UiField>
        </div>
        <UiButton @click="submitCandidate">创建候选人</UiButton>
      </UiPanel>

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
              <UiTh>阶段</UiTh>
              <UiTh>标签</UiTh>
              <UiTh>操作</UiTh>
            </tr>
          </thead>
          <tbody>
            <tr v-for="candidate in store.candidates" :key="candidate.id">
              <UiTd>{{ candidate.name }}</UiTd>
              <UiTd>
                <UiBadge :tone="stageTone(candidate.stage)">{{ formatStageLabel(candidate.stage) }}</UiBadge>
              </UiTd>
              <UiTd>{{ candidate.tags.join(" / ") || "-" }}</UiTd>
              <UiTd>
                <UiButton variant="ghost" @click="selectCandidate(candidate.id)">查看</UiButton>
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

    <div v-if="selectedCandidate" class="flex flex-col gap-4">
      <UiPanel v-if="selectedCandidate" :title="`候选人详情: ${selectedCandidate.name}`">
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

      <UiPanel v-if="selectedCandidate" title="简历录入">
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
            <select v-model="sidecarResumeSource">
              <option value="boss">Boss</option>
              <option value="zhilian">智联</option>
              <option value="wuba">58</option>
              <option value="lagou">拉勾</option>
            </select>
          </UiField>
          <UiField label="抓取模式">
            <select v-model="sidecarResumeMode">
              <option value="compliant">合规模式</option>
              <option value="advanced">高级模式</option>
            </select>
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

      <UiPanel v-if="selectedScreening.length" title="最新初筛">
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

      <UiPanel v-if="selectedAnalysis.length" title="最新分析">
        <p class="m-0 mb-2">总分: {{ selectedAnalysis[0].overallScore }}</p>
        <ul class="mt-2 pl-4.5">
          <li v-for="item in selectedAnalysis[0].dimensionScores" :key="item.key" class="mb-1.5">
            {{ item.key }}: {{ item.score }} ({{ item.reason }})
          </li>
        </ul>
        <p v-if="selectedAnalysis[0].risks.length" class="m-0 mt-2">风险: {{ selectedAnalysis[0].risks.join("；") }}</p>
      </UiPanel>

      <UiPanel v-if="selectedEvents.length" title="流转历史">
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
    </div>
  </section>
</template>
