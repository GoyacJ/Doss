<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import type { CrawlMode, PipelineStage } from "@doss/shared";
import { useRecruitingStore } from "../stores/recruiting";
import { formatStageLabel, nextStageOptions } from "../lib/pipeline";

const store = useRecruitingStore();
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
const stageNote = ref("");
const sidecarResumeSource = ref<"boss" | "zhilian" | "wuba">("boss");
const sidecarResumeMode = ref<CrawlMode>("compliant");
const sidecarResumeStatus = ref("");

const selectedCandidate = computed(() =>
  store.candidates.find((item) => item.id === selectedCandidateId.value) ?? null,
);

const selectedAnalysis = computed(() => {
  if (!selectedCandidateId.value) {
    return [];
  }
  return store.analyses[selectedCandidateId.value] ?? [];
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

async function runAnalysis() {
  if (!selectedCandidateId.value) {
    return;
  }

  await store.analyzeCandidate(selectedCandidateId.value);
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

async function doSearch() {
  await store.search(searchKeyword.value);
}

async function crawlResumeFromSidecar() {
  if (!selectedCandidate.value?.external_id) {
    return;
  }

  const source = selectedCandidate.value.source;
  if (source === "boss" || source === "zhilian" || source === "wuba") {
    sidecarResumeSource.value = source;
  }

  const response = await store.runSidecarResumeCrawl({
    source: sidecarResumeSource.value,
    mode: sidecarResumeMode.value,
    localCandidateId: selectedCandidate.value.id,
    externalCandidateId: selectedCandidate.value.external_id,
  });
  sidecarResumeStatus.value = `已抓取并写入简历，任务状态: ${response.result.status}`;
}
</script>

<template>
  <section class="page two-column">
    <div class="left-column">
      <article class="panel">
        <h3>新增候选人</h3>
        <div class="form-grid">
          <input v-model="candidateForm.name" placeholder="姓名" />
          <input v-model="candidateForm.current_company" placeholder="当前公司" />
          <input v-model.number="candidateForm.years_of_experience" type="number" min="0" step="0.5" placeholder="年限" />
          <input v-model="candidateForm.phone" placeholder="手机号" />
          <input v-model="candidateForm.email" placeholder="邮箱" />
          <input v-model="candidateForm.tags" placeholder="标签，逗号分隔" />
        </div>
        <button class="button" @click="submitCandidate">创建候选人</button>
      </article>

      <article class="panel">
        <div class="inline-row">
          <h3>候选人列表</h3>
          <input v-model="searchKeyword" placeholder="本地搜索关键词" @keyup.enter="doSearch" />
          <button class="button secondary" @click="doSearch">搜索</button>
        </div>

        <table class="table">
          <thead>
            <tr>
              <th>姓名</th>
              <th>阶段</th>
              <th>标签</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="candidate in store.candidates" :key="candidate.id">
              <td>{{ candidate.name }}</td>
              <td>{{ formatStageLabel(candidate.stage) }}</td>
              <td>{{ candidate.tags.join(" / ") || "-" }}</td>
              <td>
                <button class="button ghost" @click="selectCandidate(candidate.id)">查看</button>
              </td>
            </tr>
          </tbody>
        </table>

        <div v-if="store.searchResults.length" class="search-results">
          <h4>搜索结果</h4>
          <ul>
            <li v-for="result in store.searchResults" :key="`${result.candidate_id}-${result.snippet}`">
              #{{ result.candidate_id }} {{ result.name }} - {{ result.stage }}
              <span>{{ result.snippet }}</span>
            </li>
          </ul>
        </div>
      </article>
    </div>

    <div class="right-column">
      <article class="panel" v-if="selectedCandidate">
        <h3>候选人详情: {{ selectedCandidate.name }}</h3>
        <p>阶段: {{ formatStageLabel(selectedCandidate.stage) }}</p>
        <p>电话: {{ selectedCandidate.phone_masked || "-" }}</p>
        <p>邮箱: {{ selectedCandidate.email_masked || "-" }}</p>

        <div class="inline-row">
          <input v-model="stageNote" placeholder="阶段变更备注" />
        </div>

        <div class="stage-actions">
          <button
            v-for="stage in nextStageOptions(selectedCandidate.stage)"
            :key="stage"
            class="button secondary"
            @click="moveStage(stage)"
          >
            迁移到 {{ formatStageLabel(stage) }}
          </button>
        </div>
      </article>

      <article class="panel" v-if="selectedCandidate">
        <h3>简历录入</h3>
        <textarea v-model="resumeText" rows="6" placeholder="粘贴简历文本"></textarea>
        <input v-model="resumeSkills" placeholder="技能列表，逗号分隔" />
        <div class="form-grid">
          <select v-model="sidecarResumeSource">
            <option value="boss">Boss</option>
            <option value="zhilian">智联</option>
            <option value="wuba">58</option>
          </select>
          <select v-model="sidecarResumeMode">
            <option value="compliant">合规模式</option>
            <option value="advanced">高级模式</option>
          </select>
        </div>
        <div class="inline-row">
          <button class="button" @click="saveResume">保存简历</button>
          <button
            class="button secondary"
            :disabled="!selectedCandidate.external_id"
            @click="crawlResumeFromSidecar"
          >
            Sidecar抓取简历并入库
          </button>
          <button class="button" @click="runAnalysis">运行AI分析</button>
        </div>
        <p v-if="!selectedCandidate.external_id">当前候选人无外部ID，无法从招聘站抓取简历。</p>
        <p v-if="sidecarResumeStatus">{{ sidecarResumeStatus }}</p>
      </article>

      <article class="panel" v-if="selectedAnalysis.length">
        <h3>最新分析</h3>
        <p>总分: {{ selectedAnalysis[0].overallScore }}</p>
        <ul>
          <li v-for="item in selectedAnalysis[0].dimensionScores" :key="item.key">
            {{ item.key }}: {{ item.score }} ({{ item.reason }})
          </li>
        </ul>
        <p v-if="selectedAnalysis[0].risks.length">风险: {{ selectedAnalysis[0].risks.join("；") }}</p>
      </article>

      <article class="panel" v-if="selectedEvents.length">
        <h3>流转历史</h3>
        <ul>
          <li v-for="item in selectedEvents" :key="item.id">
            {{ item.from_stage }} → {{ item.to_stage }} ({{ item.created_at }})
          </li>
        </ul>
      </article>
    </div>
  </section>
</template>
