<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import type { JobRecord, SortRule } from "@doss/shared";
import type { ScoringItemConfig, ScoringTemplateConfig, ScoringTemplateRecord } from "../services/backend";
import { jobStatusLabel, jobStatusTone } from "../lib/status";
import {
  resolveOverrideScoringTemplateOptions,
  resolveResidentDefaultScoringTemplate,
} from "../lib/scoring-template-options";
import { useRecruitingStore } from "../stores/recruiting";
import UiBadge from "../components/UiBadge.vue";
import UiButton from "../components/UiButton.vue";
import UiField from "../components/UiField.vue";
import UiPanel from "../components/UiPanel.vue";
import UiSelect from "../components/UiSelect.vue";
import UiTableFilterPanel from "../components/UiTableFilterPanel.vue";
import UiTable from "../components/UiTable.vue";
import UiTablePagination from "../components/UiTablePagination.vue";
import UiTableToolbar from "../components/UiTableToolbar.vue";
import UiTd from "../components/UiTd.vue";
import UiTh from "../components/UiTh.vue";
import { includesKeyword } from "../lib/table-filter";
import { clampPage, paginateRows } from "../lib/table-pagination";
import { normalizeSortRules, sortRowsByRules, type SortResolver } from "../lib/table-sort";
import { useToastStore } from "../stores/toast";

const store = useRecruitingStore();
const toast = useToastStore();

const jobModalOpen = ref(false);
const jobModalMode = ref<"create" | "edit">("create");
const editingJobId = ref<number | null>(null);
const savingJob = ref(false);
const stoppingJobId = ref<number | null>(null);
const deletingJobId = ref<number | null>(null);
const deleteConfirmJob = ref<JobRecord | null>(null);

const templateListModalOpen = ref(false);
const templateEditorOpen = ref(false);
const templateEditorMode = ref<"create" | "edit">("create");
const savingTemplate = ref(false);
const deletingTemplateId = ref<number | null>(null);
const deleteConfirmTemplate = ref<ScoringTemplateRecord | null>(null);
const jobsAdvancedFilterOpen = ref(false);

const jobForm = reactive({
  title: "",
  company: "",
  city: "",
  salary_k: "",
  description: "",
  template_id: 0,
});

const templateForm = reactive<{
  template_id: number;
  name: string;
  config: ScoringTemplateConfig;
}>({
  template_id: 0,
  name: "",
  config: {
    weights: { t0: 50, t1: 30, t2: 10, t3: 10 },
    t0: { items: [] },
    t1: { items: [] },
    t2: { items: [] },
    t3: { items: [] },
  },
});

const jobTableFilters = reactive({
  quickKeyword: "",
  company: "",
  city: "",
  status: "" as "" | "ACTIVE" | "STOPPED",
});

const templateTableFilters = reactive({
  quickKeyword: "",
});
const jobPage = ref(1);
const jobPageSize = ref(10);
const templatePage = ref(1);
const templatePageSize = ref(10);

const sectionKeys = ["t0", "t1", "t2", "t3"] as const;
type SectionKey = typeof sectionKeys[number];

const templateWeightTotal = computed(() =>
  Number(templateForm.config.weights.t0 || 0)
  + Number(templateForm.config.weights.t1 || 0)
  + Number(templateForm.config.weights.t2 || 0)
  + Number(templateForm.config.weights.t3 || 0),
);
const residentDefaultTemplate = computed(() => resolveResidentDefaultScoringTemplate(store.scoringTemplates));
const overrideTemplateOptions = computed(() =>
  resolveOverrideScoringTemplateOptions(store.scoringTemplates),
);
const residentDefaultTemplateId = computed(() => residentDefaultTemplate.value?.id ?? null);
const isEditingResidentDefaultTemplate = computed(() =>
  templateEditorMode.value === "edit"
  && templateForm.template_id > 0
  && residentDefaultTemplateId.value === templateForm.template_id,
);

type JobSortField
  = | "title"
    | "company"
    | "city"
    | "salary_k"
    | "status"
    | "template_name"
    | "updated_at";
type TemplateSortField = "name" | "dimension_count" | "updated_at";

const jobSortOptions: { label: string; value: JobSortField }[] = [
  { label: "职位", value: "title" },
  { label: "公司", value: "company" },
  { label: "城市", value: "city" },
  { label: "薪资", value: "salary_k" },
  { label: "状态", value: "status" },
  { label: "评分模板", value: "template_name" },
  { label: "更新时间", value: "updated_at" },
];

const templateSortOptions: { label: string; value: TemplateSortField }[] = [
  { label: "模板名称", value: "name" },
  { label: "维度数", value: "dimension_count" },
  { label: "更新时间", value: "updated_at" },
];

const jobSorts = ref<SortRule<JobSortField>[]>([
  { field: "updated_at", direction: "desc" },
]);
const templateSorts = ref<SortRule<TemplateSortField>[]>([
  { field: "updated_at", direction: "desc" },
]);

const effectiveJobSorts = computed(() =>
  normalizeSortRules(
    jobSorts.value,
    jobSortOptions.map((item) => item.value),
  ),
);

const effectiveTemplateSorts = computed(() =>
  normalizeSortRules(
    templateSorts.value,
    templateSortOptions.map((item) => item.value),
  ),
);

const jobStatusOptions = [
  { value: "", label: "全部状态" },
  { value: "ACTIVE", label: "进行中" },
  { value: "STOPPED", label: "已停止" },
];

const filteredJobs = computed(() =>
  store.jobs.filter((job) => {
    if (!includesKeyword(
      jobTableFilters.quickKeyword,
      job.id,
      job.title,
      job.company,
      job.city,
      job.salary_k,
      job.scoring_template_name,
    )) {
      return false;
    }
    if (!includesKeyword(jobTableFilters.company, job.company)) {
      return false;
    }
    if (!includesKeyword(jobTableFilters.city, job.city)) {
      return false;
    }
    if (jobTableFilters.status && (job.status ?? "ACTIVE") !== jobTableFilters.status) {
      return false;
    }
    return true;
  }),
);

const jobSortResolver: SortResolver<JobRecord, JobSortField> = {
  title: (row) => row.title,
  company: (row) => row.company,
  city: (row) => row.city,
  salary_k: (row) => row.salary_k,
  status: (row) => row.status ?? "ACTIVE",
  template_name: (row) => row.scoring_template_name ?? residentDefaultTemplate.value?.name ?? "默认评分模板",
  updated_at: (row) => row.updated_at,
};

const displayJobs = computed(() =>
  sortRowsByRules(filteredJobs.value, effectiveJobSorts.value, jobSortResolver),
);
const pagedJobs = computed(() =>
  paginateRows(displayJobs.value, jobPage.value, jobPageSize.value),
);

const filteredTemplates = computed(() =>
  store.scoringTemplates.filter((template) =>
    includesKeyword(
      templateTableFilters.quickKeyword,
      template.name,
      template.id,
      template.updated_at,
    )),
);

const templateSortResolver: SortResolver<ScoringTemplateRecord, TemplateSortField> = {
  name: (row) => row.name,
  dimension_count: (row) =>
    row.config.t0.items.length
    + row.config.t1.items.length
    + row.config.t2.items.length
    + row.config.t3.items.length,
  updated_at: (row) => row.updated_at,
};

const displayTemplates = computed(() =>
  sortRowsByRules(filteredTemplates.value, effectiveTemplateSorts.value, templateSortResolver),
);
const pagedTemplates = computed(() =>
  paginateRows(displayTemplates.value, templatePage.value, templatePageSize.value),
);

function sortJobsByColumn(payload: { field: string; direction: "asc" | "desc" }) {
  const field = payload.field as JobSortField;
  if (!jobSortOptions.some((item) => item.value === field)) {
    return;
  }
  const next = [
    { field, direction: payload.direction },
    ...effectiveJobSorts.value.filter((rule) => rule.field !== field),
  ];
  jobSorts.value = normalizeSortRules(next, jobSortOptions.map((item) => item.value));
}

function sortTemplatesByColumn(payload: { field: string; direction: "asc" | "desc" }) {
  const field = payload.field as TemplateSortField;
  if (!templateSortOptions.some((item) => item.value === field)) {
    return;
  }
  const next = [
    { field, direction: payload.direction },
    ...effectiveTemplateSorts.value.filter((rule) => rule.field !== field),
  ];
  templateSorts.value = normalizeSortRules(next, templateSortOptions.map((item) => item.value));
}

function resolveErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error) {
    return error.message || fallback;
  }
  if (typeof error === "string" && error.trim()) {
    return error;
  }
  return fallback;
}

function createDefaultScoringConfig(): ScoringTemplateConfig {
  return {
    weights: { t0: 50, t1: 30, t2: 10, t3: 10 },
    t0: {
      items: [
        {
          key: "required_skills_match",
          label: "岗位技能匹配",
          description: "岗位描述/技能要求与候选人技能覆盖是否匹配。",
          weight: 50,
        },
        {
          key: "years_experience_match",
          label: "经验年限匹配",
          description: "候选人年限是否满足岗位复杂度要求。",
          weight: 30,
        },
        {
          key: "resume_completeness",
          label: "简历信息完整度",
          description: "简历证据是否足以支撑判断。",
          weight: 20,
        },
      ],
    },
    t1: {
      items: [
        { key: "goal_orientation", label: "目标导向", description: "是否有明确目标并形成可交付结果。", weight: 30 },
        { key: "team_collaboration", label: "团队协作", description: "跨角色协作、沟通与推进效率。", weight: 15 },
        { key: "self_drive", label: "自驱力", description: "主动承担、持续推进和问题闭环能力。", weight: 15 },
        { key: "reflection_iteration", label: "反思迭代", description: "复盘意识和迭代改进能力。", weight: 10 },
        { key: "openness", label: "开放性", description: "对反馈与变化的接受度和执行力。", weight: 8 },
        { key: "resilience", label: "抗压韧性", description: "复杂场景下的稳定性和恢复能力。", weight: 7 },
        { key: "learning_ability", label: "学习能力", description: "知识吸收与迁移速度。", weight: 10 },
        { key: "values_fit", label: "价值观契合", description: "与团队协作价值观一致性。", weight: 5 },
      ],
    },
    t2: {
      items: [
        {
          key: "core_skill_bonus",
          label: "核心技能加分",
          description: "核心技能命中程度是否超出岗位最低要求。",
          weight: 40,
        },
        {
          key: "project_impact_bonus",
          label: "项目影响力加分",
          description: "项目成果是否有可量化业务影响。",
          weight: 30,
        },
        {
          key: "rare_stack_bonus",
          label: "稀缺技术栈加分",
          description: "是否具备岗位稀缺/高价值技术栈。",
          weight: 30,
        },
      ],
    },
    t3: {
      items: [
        {
          key: "salary_risk",
          label: "薪资风险",
          description: "薪资预期与岗位预算差异风险（低风险高分）。",
          weight: 35,
        },
        {
          key: "stability_risk",
          label: "稳定性风险",
          description: "履历稳定性与持续投入风险（低风险高分）。",
          weight: 35,
        },
        {
          key: "info_completeness_risk",
          label: "信息缺失风险",
          description: "关键信息缺失带来的决策风险（低风险高分）。",
          weight: 30,
        },
      ],
    },
  };
}

function createDefaultSectionItem(section: SectionKey, index: number): ScoringItemConfig {
  return {
    key: `${section}_item_${index}`,
    label: `${section.toUpperCase()} 指标${index}`,
    description: "",
    weight: 10,
  };
}

function cloneScoringConfig(config: ScoringTemplateConfig): ScoringTemplateConfig {
  return JSON.parse(JSON.stringify(config)) as ScoringTemplateConfig;
}

function sectionWeightTotal(sectionKey: SectionKey): number {
  return templateForm.config[sectionKey].items.reduce((sum, item) => sum + Number(item.weight || 0), 0);
}

function resetJobForm() {
  editingJobId.value = null;
  jobForm.title = "";
  jobForm.company = "";
  jobForm.city = "";
  jobForm.salary_k = "";
  jobForm.description = "";
  jobForm.template_id = 0;
}

function resetTemplateForm() {
  templateForm.template_id = 0;
  templateForm.name = "";
  templateForm.config = createDefaultScoringConfig();
}

function toOptionalText(value: string): string | undefined {
  const trimmed = value.trim();
  return trimmed || undefined;
}

function normalizeJobTemplateId(templateId: number | null | undefined): number {
  if (!templateId || templateId <= 0) {
    return 0;
  }

  return overrideTemplateOptions.value.some((item) => item.id === templateId)
    ? templateId
    : 0;
}

function isResidentDefaultTemplate(template: ScoringTemplateRecord): boolean {
  return residentDefaultTemplateId.value === template.id;
}

function openCreateJobModal() {
  jobModalMode.value = "create";
  resetJobForm();
  jobModalOpen.value = true;
}

function openEditJobModal(job: JobRecord) {
  jobModalMode.value = "edit";
  editingJobId.value = job.id;
  jobForm.title = job.title;
  jobForm.company = job.company;
  jobForm.city = job.city || "";
  jobForm.salary_k = job.salary_k || "";
  jobForm.description = job.description || "";
  const templateId = job.scoring_template_id ?? 0;
  jobForm.template_id = store.scoringTemplates.length > 0
    ? normalizeJobTemplateId(templateId)
    : templateId;
  jobModalOpen.value = true;
}

function closeJobModal(force = false) {
  if (savingJob.value && !force) {
    return;
  }
  jobModalOpen.value = false;
}

async function saveJob() {
  const title = jobForm.title.trim();
  const company = jobForm.company.trim();
  if (!title || !company) {
    toast.warning("请填写职位名称和公司");
    return;
  }

  savingJob.value = true;
  try {
    let savedJob: JobRecord;
    if (jobModalMode.value === "create") {
      savedJob = await store.addJob({
        title,
        company,
        city: toOptionalText(jobForm.city),
        salary_k: toOptionalText(jobForm.salary_k),
        description: toOptionalText(jobForm.description),
      });
    } else {
      const jobId = editingJobId.value;
      if (!jobId) {
        toast.danger("职位ID缺失");
        return;
      }
      savedJob = await store.updateJob({
        job_id: jobId,
        title,
        company,
        city: toOptionalText(jobForm.city),
        salary_k: toOptionalText(jobForm.salary_k),
        description: toOptionalText(jobForm.description),
      });
    }

    await store.setJobScoringTemplate({
      job_id: savedJob.id,
      template_id: jobForm.template_id > 0 ? jobForm.template_id : null,
    });

    toast.success(jobModalMode.value === "create" ? "职位已创建" : "职位已更新");
    closeJobModal(true);
    resetJobForm();
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "保存职位失败"));
  } finally {
    savingJob.value = false;
  }
}

async function stopJob(job: JobRecord) {
  if (job.status === "STOPPED") {
    return;
  }

  stoppingJobId.value = job.id;
  try {
    await store.stopJob(job.id);
    toast.success("职位已停止");
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "停止职位失败"));
  } finally {
    stoppingJobId.value = null;
  }
}

function askRemoveJob(job: JobRecord) {
  deleteConfirmJob.value = job;
}

function cancelRemoveJob() {
  if (deletingJobId.value) {
    return;
  }
  deleteConfirmJob.value = null;
}

async function removeJob() {
  const job = deleteConfirmJob.value;
  if (!job) {
    return;
  }

  deletingJobId.value = job.id;
  try {
    await store.deleteJob(job.id);
    deleteConfirmJob.value = null;
    toast.success("职位已删除");
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "删除职位失败"));
  } finally {
    deletingJobId.value = null;
  }
}

function closeTemplateListModal(force = false) {
  if (savingTemplate.value && !force) {
    return;
  }
  templateEditorOpen.value = false;
  templateListModalOpen.value = false;
}

function openTemplateListModal() {
  templateEditorOpen.value = false;
  templateListModalOpen.value = true;
}

function closeTemplateEditor(force = false) {
  if (savingTemplate.value && !force) {
    return;
  }
  templateEditorOpen.value = false;
}

function openCreateTemplateEditor() {
  templateEditorMode.value = "create";
  resetTemplateForm();
  templateEditorOpen.value = true;
}

function openEditTemplateEditor(template: ScoringTemplateRecord) {
  templateEditorMode.value = "edit";
  templateForm.template_id = template.id;
  templateForm.name = template.name || "";
  templateForm.config = cloneScoringConfig(template.config);
  templateEditorOpen.value = true;
}

function handleTemplateDrawerBackdropClick() {
  if (templateEditorOpen.value) {
    closeTemplateEditor();
    return;
  }
  closeTemplateListModal();
}

function sectionLabel(section: SectionKey): string {
  if (section === "t0") {
    return "T0 重要指标";
  }
  if (section === "t1") {
    return "T1 指标配置";
  }
  if (section === "t2") {
    return "T2 加分项";
  }
  return "T3 风险项";
}

function addTemplateItem(section: SectionKey) {
  const next = templateForm.config[section].items.length + 1;
  templateForm.config[section].items.push(createDefaultSectionItem(section, next));
}

function removeTemplateItem(section: SectionKey, index: number) {
  if (templateForm.config[section].items.length <= 1) {
    toast.warning(`${sectionLabel(section)} 至少保留一个条目`);
    return;
  }
  templateForm.config[section].items.splice(index, 1);
}

async function saveTemplate() {
  if (templateWeightTotal.value !== 100) {
    toast.warning(`区块权重总和必须为 100，当前为 ${templateWeightTotal.value}`);
    return;
  }

  const normalizedConfig = cloneScoringConfig(templateForm.config);
  for (const section of sectionKeys) {
    if (normalizedConfig[section].items.length === 0) {
      toast.warning(`${sectionLabel(section)} 至少保留一个条目`);
      return;
    }
    let sectionSum = 0;
    for (const item of normalizedConfig[section].items) {
      item.key = item.key.trim();
      item.label = item.label.trim();
      item.description = item.description.trim();
      item.weight = Number(item.weight);
      if (!item.key || !item.label) {
        toast.warning(`${sectionLabel(section)} 请填写完整的条目 key 与名称`);
        return;
      }
      if (!Number.isFinite(item.weight) || item.weight <= 0) {
        toast.warning(`${sectionLabel(section)} 条目权重必须大于 0`);
        return;
      }
      sectionSum += item.weight;
    }
    if (sectionSum !== 100) {
      toast.warning(`${sectionLabel(section)} 条目权重总和必须为 100，当前为 ${sectionSum}`);
      return;
    }
  }

  savingTemplate.value = true;
  try {
    if (templateEditorMode.value === "create") {
      await store.createScoringTemplate({
        name: templateForm.name.trim() || "新评分模板",
        config: normalizedConfig,
      });
      toast.success("评分模板已创建");
    } else {
      if (!templateForm.template_id) {
        toast.danger("模板ID缺失");
        return;
      }
      await store.updateScoringTemplate({
        template_id: templateForm.template_id,
        name: templateForm.name.trim() || undefined,
        config: normalizedConfig,
      });
      toast.success("评分模板已更新");
    }

    await store.loadScoringTemplates();
    closeTemplateEditor(true);
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "保存评分模板失败"));
  } finally {
    savingTemplate.value = false;
  }
}

function askRemoveTemplate(template: ScoringTemplateRecord) {
  if (isResidentDefaultTemplate(template)) {
    toast.warning("默认评分模板不可删除，可直接编辑");
    return;
  }
  deleteConfirmTemplate.value = template;
}

function cancelRemoveTemplate() {
  if (deletingTemplateId.value) {
    return;
  }
  deleteConfirmTemplate.value = null;
}

async function removeTemplate() {
  const template = deleteConfirmTemplate.value;
  if (!template) {
    return;
  }

  deletingTemplateId.value = template.id;
  try {
    await store.deleteScoringTemplate(template.id);
    if (jobForm.template_id === template.id) {
      jobForm.template_id = 0;
    }
    deleteConfirmTemplate.value = null;
    toast.success("评分模板已删除");
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "删除评分模板失败"));
  } finally {
    deletingTemplateId.value = null;
  }
}

watch(
  () => store.scoringTemplates,
  () => {
    if (!jobModalOpen.value || store.scoringTemplates.length === 0) {
      return;
    }
    jobForm.template_id = normalizeJobTemplateId(jobForm.template_id);
  },
  { deep: true },
);

watch(
  () => [
    jobTableFilters.quickKeyword,
    jobTableFilters.company,
    jobTableFilters.city,
    jobTableFilters.status,
    JSON.stringify(effectiveJobSorts.value),
  ],
  () => {
    jobPage.value = 1;
  },
);

watch(
  () => [templateTableFilters.quickKeyword, JSON.stringify(effectiveTemplateSorts.value)],
  () => {
    templatePage.value = 1;
  },
);

watch(jobPageSize, () => {
  jobPage.value = 1;
});

watch(templatePageSize, () => {
  templatePage.value = 1;
});

watch(
  () => displayJobs.value.length,
  (total) => {
    jobPage.value = clampPage(jobPage.value, total, jobPageSize.value);
  },
  { immediate: true },
);

watch(
  () => displayTemplates.value.length,
  (total) => {
    templatePage.value = clampPage(templatePage.value, total, templatePageSize.value);
  },
  { immediate: true },
);

onMounted(async () => {
  try {
    await store.loadScoringTemplates();
  } catch (error) {
    toast.danger(resolveErrorMessage(error, "加载评分模板失败"));
  }
});
</script>

<template>
  <section class="flex flex-col gap-4">
    <header class="flex items-center gap-3">
      <h2 class="text-2xl font-700">职位池</h2>
    </header>

    <UiPanel>
      <template #header>
        <div class="mb-1 flex items-center justify-between gap-3 flex-wrap">
          <input
            v-model="jobTableFilters.quickKeyword"
            class="jobs-header-input w-full max-w-80 lt-sm:max-w-full"
            placeholder="输入职位/公司/城市关键词"
          />
          <div class="flex items-center gap-2">
            <UiButton @click="openCreateJobModal">创建职位</UiButton>
            <UiButton variant="secondary" @click="openTemplateListModal">评分模板</UiButton>
          </div>
        </div>
      </template>

      <UiTableFilterPanel v-model:open="jobsAdvancedFilterOpen">
        <div class="grid grid-cols-3 gap-2.5 lt-lg:grid-cols-1">
          <UiField label="公司">
            <input v-model="jobTableFilters.company" placeholder="按公司筛选" />
          </UiField>
          <UiField label="城市">
            <input v-model="jobTableFilters.city" placeholder="按城市筛选" />
          </UiField>
          <UiField label="状态">
            <UiSelect v-model="jobTableFilters.status" :options="jobStatusOptions" />
          </UiField>
        </div>
      </UiTableFilterPanel>

      <UiTable spacing="comfortable">
        <thead>
          <tr>
            <UiTh sort-field="title" :sorts="effectiveJobSorts" @sort="sortJobsByColumn">职位</UiTh>
            <UiTh sort-field="company" :sorts="effectiveJobSorts" @sort="sortJobsByColumn">公司</UiTh>
            <UiTh sort-field="city" :sorts="effectiveJobSorts" @sort="sortJobsByColumn">城市</UiTh>
            <UiTh sort-field="salary_k" :sorts="effectiveJobSorts" @sort="sortJobsByColumn">薪资</UiTh>
            <UiTh sort-field="status" :sorts="effectiveJobSorts" @sort="sortJobsByColumn">状态</UiTh>
            <UiTh sort-field="template_name" :sorts="effectiveJobSorts" @sort="sortJobsByColumn">评分模板</UiTh>
            <UiTh sort-field="updated_at" :sorts="effectiveJobSorts" @sort="sortJobsByColumn">更新时间</UiTh>
            <UiTh>操作</UiTh>
          </tr>
        </thead>
        <tbody>
          <tr v-for="job in pagedJobs" :key="job.id">
            <UiTd>{{ job.title }}</UiTd>
            <UiTd>{{ job.company }}</UiTd>
            <UiTd>{{ job.city || "-" }}</UiTd>
            <UiTd>{{ job.salary_k || "-" }}</UiTd>
            <UiTd>
              <UiBadge :tone="jobStatusTone(job.status)">{{ jobStatusLabel(job.status) }}</UiBadge>
            </UiTd>
            <UiTd>{{ job.scoring_template_name || residentDefaultTemplate?.name || "默认评分模板" }}</UiTd>
            <UiTd no-wrap>{{ job.updated_at }}</UiTd>
            <UiTd no-wrap>
              <div class="flex justify-center gap-2 flex-wrap">
                <UiButton variant="ghost" size="sm" @click="openEditJobModal(job)">编辑</UiButton>
                <UiButton
                  variant="ghost"
                  size="sm"
                  :disabled="job.status === 'STOPPED' || stoppingJobId === job.id"
                  @click="stopJob(job)"
                >
                  {{ stoppingJobId === job.id ? "处理中..." : (job.status === "STOPPED" ? "已停止" : "停止") }}
                </UiButton>
                <UiButton
                  variant="ghost"
                  size="sm"
                  :disabled="deletingJobId === job.id"
                  @click="askRemoveJob(job)"
                >
                  {{ deletingJobId === job.id ? "删除中..." : "删除" }}
                </UiButton>
              </div>
            </UiTd>
          </tr>
          <tr v-if="pagedJobs.length === 0">
            <UiTd colspan="8" class="text-center text-muted py-6">暂无职位</UiTd>
          </tr>
        </tbody>
      </UiTable>
      <UiTablePagination
        v-model:page="jobPage"
        v-model:page-size="jobPageSize"
        :total="displayJobs.length"
      />
    </UiPanel>

    <div
      v-if="jobModalOpen"
      class="fixed inset-0 z-[80] flex items-center justify-center bg-black/35 p-4"
      @click.self="closeJobModal()"
    >
      <div class="w-full max-w-3xl">
        <UiPanel :title="jobModalMode === 'create' ? '创建职位' : '编辑职位'">
          <div class="grid grid-cols-2 gap-2.5 lt-lg:grid-cols-1">
            <UiField label="职位名称">
              <input v-model="jobForm.title" placeholder="例如：高级前端工程师" />
            </UiField>
            <UiField label="公司">
              <input v-model="jobForm.company" placeholder="公司名称" />
            </UiField>
            <UiField label="城市">
              <input v-model="jobForm.city" placeholder="工作城市" />
            </UiField>
            <UiField label="薪资区间(k)">
              <input v-model="jobForm.salary_k" placeholder="例如：30-45" />
            </UiField>
          </div>
          <UiField class="mt-2.5" label="评分模板">
            <select v-model.number="jobForm.template_id">
              <option :value="0">{{ residentDefaultTemplate?.name || "默认评分模板" }}</option>
              <option v-for="template in overrideTemplateOptions" :key="template.id" :value="template.id">
                {{ template.name }}
              </option>
            </select>
          </UiField>
          <UiField class="mt-2.5" label="岗位描述 / 技能要求">
            <textarea v-model="jobForm.description" placeholder="岗位职责、技术栈、加分项" />
          </UiField>

          <div class="mt-4 flex flex-wrap justify-end gap-2">
            <UiButton variant="ghost" :disabled="savingJob" @click="closeJobModal()">取消</UiButton>
            <UiButton :disabled="savingJob" @click="saveJob">
              {{ savingJob ? "保存中..." : (jobModalMode === "create" ? "创建职位" : "保存修改") }}
            </UiButton>
          </div>
        </UiPanel>
      </div>
    </div>

    <div
      v-if="templateListModalOpen"
      class="fixed inset-0 z-[82] flex justify-end bg-black/35"
      @click.self="handleTemplateDrawerBackdropClick()"
    >
      <div class="h-full w-full max-w-3xl p-4 pl-0 lt-lg:max-w-full lt-lg:p-0">
        <UiPanel class="h-full flex flex-col">
          <template #header>
            <div class="mb-2 flex items-center justify-between gap-3">
              <div class="flex items-center gap-2">
                <UiButton
                  v-if="templateEditorOpen"
                  variant="ghost"
                  :disabled="savingTemplate"
                  @click="closeTemplateEditor()"
                >
                  返回列表
                </UiButton>
                <h3 class="text-lg font-700">
                  {{ templateEditorOpen ? (templateEditorMode === "create" ? "创建评分模板" : "编辑评分模板") : "评分模板" }}
                </h3>
              </div>
              <div class="flex items-center gap-2">
                <UiButton
                  v-if="!templateEditorOpen"
                  variant="secondary"
                  @click="openCreateTemplateEditor"
                >
                  创建模板
                </UiButton>
                <UiButton variant="ghost" :disabled="savingTemplate" @click="closeTemplateListModal()">关闭</UiButton>
              </div>
            </div>
          </template>

          <div v-if="!templateEditorOpen" class="min-h-0 flex-1 overflow-auto">
            <UiTableToolbar
              v-model:quick-keyword="templateTableFilters.quickKeyword"
              :show-advanced-toggle="false"
              :show-refresh="false"
              :show-apply="false"
              quick-placeholder="输入模板关键词"
            />

            <UiTable spacing="comfortable">
              <thead>
                <tr>
                  <UiTh sort-field="name" :sorts="effectiveTemplateSorts" @sort="sortTemplatesByColumn">模板名称</UiTh>
                  <UiTh sort-field="dimension_count" :sorts="effectiveTemplateSorts" @sort="sortTemplatesByColumn">维度数</UiTh>
                  <UiTh sort-field="updated_at" :sorts="effectiveTemplateSorts" @sort="sortTemplatesByColumn">更新时间</UiTh>
                  <UiTh>操作</UiTh>
                </tr>
              </thead>
              <tbody>
                <tr v-for="template in pagedTemplates" :key="template.id">
                  <UiTd>
                    {{ template.name }}
                    <span v-if="isResidentDefaultTemplate(template)" class="ml-1 text-xs text-muted">(默认)</span>
                  </UiTd>
                  <UiTd>
                    {{ template.config.t0.items.length + template.config.t1.items.length + template.config.t2.items.length + template.config.t3.items.length }}
                  </UiTd>
                  <UiTd no-wrap>{{ template.updated_at }}</UiTd>
                  <UiTd no-wrap>
                    <div class="flex justify-center gap-2 flex-wrap">
                      <UiButton variant="ghost" size="sm" @click="openEditTemplateEditor(template)">编辑</UiButton>
                      <UiButton
                        variant="ghost"
                        size="sm"
                        :disabled="isResidentDefaultTemplate(template) || deletingTemplateId === template.id"
                        @click="askRemoveTemplate(template)"
                      >
                        {{ isResidentDefaultTemplate(template) ? "不可删" : (deletingTemplateId === template.id ? "删除中..." : "删除") }}
                      </UiButton>
                    </div>
                  </UiTd>
                </tr>
                <tr v-if="pagedTemplates.length === 0">
                  <UiTd colspan="4" class="text-center text-muted py-6">暂无评分模板</UiTd>
                </tr>
              </tbody>
            </UiTable>
            <UiTablePagination
              v-model:page="templatePage"
              v-model:page-size="templatePageSize"
              :total="displayTemplates.length"
            />
          </div>

          <div v-else class="min-h-0 flex-1 overflow-y-auto pr-1">
            <UiField label="模板名称">
              <input v-model="templateForm.name" placeholder="例如：前端工程师模板" />
            </UiField>
            <p v-if="isEditingResidentDefaultTemplate" class="mt-1 text-sm text-muted">
              当前为系统默认评分模板，常驻不可删除，可直接编辑四区块配置。
            </p>

            <div class="mt-3 rounded-xl border border-line p-3">
              <p class="m-0 text-sm font-700">区块权重（总和必须=100）</p>
              <div class="mt-2 grid grid-cols-2 gap-2 lt-lg:grid-cols-1">
                <UiField label="T0 权重">
                  <input v-model.number="templateForm.config.weights.t0" type="number" min="1" max="100" step="1" />
                </UiField>
                <UiField label="T1 权重">
                  <input v-model.number="templateForm.config.weights.t1" type="number" min="1" max="100" step="1" />
                </UiField>
                <UiField label="T2 权重">
                  <input v-model.number="templateForm.config.weights.t2" type="number" min="1" max="100" step="1" />
                </UiField>
                <UiField label="T3 权重">
                  <input v-model.number="templateForm.config.weights.t3" type="number" min="1" max="100" step="1" />
                </UiField>
              </div>
              <p class="mt-2 mb-0 text-sm" :class="templateWeightTotal === 100 ? 'text-brand' : 'text-danger'">
                区块权重合计: {{ templateWeightTotal }} / 100
              </p>
            </div>

            <div class="mt-3 grid gap-3">
              <div
                v-for="section in sectionKeys"
                :key="section"
                class="border border-line rounded-xl p-3"
              >
                <div class="flex items-center justify-between gap-2 flex-wrap">
                  <p class="m-0 text-sm font-700">{{ sectionLabel(section) }}</p>
                  <div class="flex items-center gap-2">
                    <UiButton variant="secondary" @click="addTemplateItem(section)">新增条目</UiButton>
                    <span
                      class="text-xs"
                      :class="sectionWeightTotal(section) === 100 ? 'text-muted' : 'text-danger'"
                    >
                      条目权重合计 {{ sectionWeightTotal(section) }} / 100
                    </span>
                  </div>
                </div>
                <div class="mt-2 grid gap-2">
                  <div
                    v-for="(item, index) in templateForm.config[section].items"
                    :key="`${section}-${item.key}-${index}`"
                    class="border border-line rounded-lg p-2 grid grid-cols-[1fr_1fr_1fr_120px_auto] gap-2 lt-lg:grid-cols-1"
                  >
                    <UiField label="Key">
                      <input v-model="item.key" :placeholder="`${section}_item_key`" />
                    </UiField>
                    <UiField label="名称">
                      <input v-model="item.label" placeholder="条目名称" />
                    </UiField>
                    <UiField label="说明">
                      <input v-model="item.description" placeholder="条目说明" />
                    </UiField>
                    <UiField label="权重">
                      <input v-model.number="item.weight" type="number" min="1" max="100" step="1" />
                    </UiField>
                    <div class="flex items-end">
                      <UiButton variant="ghost" @click="removeTemplateItem(section, index)">删除</UiButton>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>

          <div v-if="templateEditorOpen" class="mt-4 flex flex-wrap justify-end gap-2">
            <UiButton variant="ghost" :disabled="savingTemplate" @click="closeTemplateEditor()">取消</UiButton>
            <UiButton :disabled="savingTemplate" @click="saveTemplate">
              {{ savingTemplate ? "保存中..." : "保存模板" }}
            </UiButton>
          </div>
        </UiPanel>
      </div>
    </div>

    <div
      v-if="deleteConfirmJob"
      class="fixed inset-0 z-[85] flex items-center justify-center bg-black/35 p-4"
      @click.self="cancelRemoveJob()"
    >
      <div class="w-full max-w-md">
        <UiPanel title="删除职位">
          <p class="m-0">
            确认删除职位「{{ deleteConfirmJob.title }}」吗？此操作不可撤销。
          </p>
          <div class="mt-4 flex flex-wrap justify-end gap-2">
            <UiButton
              variant="ghost"
              :disabled="deletingJobId === deleteConfirmJob.id"
              @click="cancelRemoveJob()"
            >
              取消
            </UiButton>
            <UiButton
              :disabled="deletingJobId === deleteConfirmJob.id"
              @click="removeJob()"
            >
              {{ deletingJobId === deleteConfirmJob.id ? "删除中..." : "确认删除" }}
            </UiButton>
          </div>
        </UiPanel>
      </div>
    </div>

    <div
      v-if="deleteConfirmTemplate"
      class="fixed inset-0 z-[86] flex items-center justify-center bg-black/35 p-4"
      @click.self="cancelRemoveTemplate()"
    >
      <div class="w-full max-w-md">
        <UiPanel title="删除评分模板">
          <p class="m-0">
            确认删除评分模板「{{ deleteConfirmTemplate.name }}」吗？此操作不可撤销。
          </p>
          <div class="mt-4 flex flex-wrap justify-end gap-2">
            <UiButton
              variant="ghost"
              :disabled="deletingTemplateId === deleteConfirmTemplate.id"
              @click="cancelRemoveTemplate()"
            >
              取消
            </UiButton>
            <UiButton
              :disabled="deletingTemplateId === deleteConfirmTemplate.id"
              @click="removeTemplate()"
            >
              {{ deletingTemplateId === deleteConfirmTemplate.id ? "删除中..." : "确认删除" }}
            </UiButton>
          </div>
        </UiPanel>
      </div>
    </div>
  </section>
</template>

<style scoped>
.jobs-header-input {
  min-height: 40px;
  padding-top: 8px;
  padding-bottom: 8px;
}
</style>
