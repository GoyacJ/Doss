<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import { useRecruitingStore } from "../stores/recruiting";
import UiButton from "../components/UiButton.vue";
import UiField from "../components/UiField.vue";
import UiPanel from "../components/UiPanel.vue";
import UiTable from "../components/UiTable.vue";
import UiTd from "../components/UiTd.vue";
import UiTh from "../components/UiTh.vue";
import { useToastStore } from "../stores/toast";

const store = useRecruitingStore();
const toast = useToastStore();

const form = reactive({
  title: "",
  company: "",
  city: "",
  salary_k: "",
  description: "",
});

const jobTemplateJobId = ref(0);
const jobTemplateName = ref("岗位微调模板");
const jobTemplateDimensions = ref([
  { key: "goal_orientation", label: "目标导向", weight: 30 },
  { key: "team_collaboration", label: "团队协作", weight: 15 },
  { key: "self_drive", label: "自驱力", weight: 15 },
  { key: "reflection_iteration", label: "反思迭代", weight: 10 },
  { key: "openness", label: "开放性", weight: 8 },
  { key: "resilience", label: "抗压韧性", weight: 7 },
  { key: "learning_ability", label: "学习能力", weight: 10 },
  { key: "values_fit", label: "价值观契合", weight: 5 },
]);
const jobTemplateRiskRulesText = ref("{}");

const jobTemplateWeightTotal = computed(() =>
  jobTemplateDimensions.value.reduce((sum, item) => sum + Number(item.weight || 0), 0),
);

async function submit() {
  if (!form.title || !form.company) {
    return;
  }

  await store.addJob({
    title: form.title,
    company: form.company,
    city: form.city || undefined,
    salary_k: form.salary_k || undefined,
    description: form.description || undefined,
  });

  form.title = "";
  form.company = "";
  form.city = "";
  form.salary_k = "";
  form.description = "";
}

async function loadJobTemplateOverride() {
  if (!jobTemplateJobId.value) {
    toast.warning("请先选择职位");
    return;
  }

  const template = await store.loadScreeningTemplate(jobTemplateJobId.value);
  jobTemplateName.value = template.name || `岗位 ${jobTemplateJobId.value} 微调模板`;
  if (template.dimensions.length > 0) {
    jobTemplateDimensions.value = template.dimensions.map((item) => ({
      key: item.key,
      label: item.label,
      weight: item.weight,
    }));
  }
  jobTemplateRiskRulesText.value = JSON.stringify(template.risk_rules ?? {}, null, 2);
}

async function saveJobTemplateOverride() {
  if (!jobTemplateJobId.value) {
    toast.warning("请先选择职位");
    return;
  }
  if (jobTemplateWeightTotal.value !== 100) {
    toast.warning(`权重总和必须为100，当前为 ${jobTemplateWeightTotal.value}`);
    return;
  }

  const hasInvalidDimension = jobTemplateDimensions.value.some(
    (item) => !item.key.trim() || !item.label.trim(),
  );
  if (hasInvalidDimension) {
    toast.warning("请填写完整的维度 key 与名称");
    return;
  }

  let riskRules: Record<string, unknown> = {};
  const riskRulesText = jobTemplateRiskRulesText.value.trim();
  if (riskRulesText) {
    try {
      const parsed = JSON.parse(riskRulesText);
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
        toast.warning("风险规则必须是 JSON 对象");
        return;
      }
      riskRules = parsed as Record<string, unknown>;
    } catch {
      toast.warning("风险规则 JSON 格式不正确");
      return;
    }
  }

  await store.saveScreeningTemplate({
    job_id: jobTemplateJobId.value,
    name: jobTemplateName.value.trim() || `岗位 ${jobTemplateJobId.value} 微调模板`,
    dimensions: jobTemplateDimensions.value.map((item) => ({
      key: item.key.trim(),
      label: item.label.trim(),
      weight: Number(item.weight),
    })),
    risk_rules: riskRules,
  });
  toast.success("职位微调模板已保存");
}

function addJobTemplateDimension() {
  const next = jobTemplateDimensions.value.length + 1;
  jobTemplateDimensions.value.push({
    key: `custom_dimension_${next}`,
    label: `自定义维度${next}`,
    weight: 5,
  });
}

function removeJobTemplateDimension(index: number) {
  if (jobTemplateDimensions.value.length <= 1) {
    toast.warning("至少保留一个维度");
    return;
  }
  jobTemplateDimensions.value.splice(index, 1);
}
</script>

<template>
  <section class="flex flex-col gap-4">
    <header class="flex items-center justify-between gap-3">
      <h2 class="text-2xl font-700">职位池</h2>
    </header>

    <UiPanel title="创建职位">
      <div class="grid grid-cols-2 gap-2.5 mb-2.5 lt-lg:grid-cols-1">
        <UiField label="职位名称">
          <input v-model="form.title" placeholder="例如：高级前端工程师" />
        </UiField>
        <UiField label="公司">
          <input v-model="form.company" placeholder="公司名称" />
        </UiField>
        <UiField label="城市">
          <input v-model="form.city" placeholder="工作城市" />
        </UiField>
        <UiField label="薪资区间(k)">
          <input v-model="form.salary_k" placeholder="例如：30-45" />
        </UiField>
      </div>
      <UiField label="岗位描述 / 技能要求">
        <textarea v-model="form.description" placeholder="岗位职责、技术栈、加分项" class="mb-2.5" />
      </UiField>
      <UiButton @click="submit">保存职位</UiButton>
    </UiPanel>

    <UiPanel title="评分模板微调（按职位）">
      <div class="grid grid-cols-2 gap-2.5 mb-2.5 lt-lg:grid-cols-1">
        <UiField label="选择职位">
          <select v-model.number="jobTemplateJobId">
            <option :value="0">选择职位</option>
            <option v-for="job in store.jobs" :key="job.id" :value="job.id">
              #{{ job.id }} {{ job.title }} / {{ job.company }}
            </option>
          </select>
        </UiField>
        <UiField label="模板名称">
          <input v-model="jobTemplateName" placeholder="岗位微调模板" />
        </UiField>
      </div>
      <div class="flex items-center gap-2 mb-2.5">
        <UiButton variant="secondary" @click="addJobTemplateDimension">新增维度</UiButton>
      </div>
      <div class="grid gap-2.5 mb-2.5">
        <div
          v-for="(item, index) in jobTemplateDimensions"
          :key="`${item.key}-${index}`"
          class="border border-line rounded-xl p-2.5 grid grid-cols-[1fr_1fr_140px_auto] gap-2 lt-lg:grid-cols-1"
        >
          <UiField label="维度 Key">
            <input v-model="item.key" placeholder="例如：goal_orientation" />
          </UiField>
          <UiField label="维度名称">
            <input v-model="item.label" placeholder="例如：目标导向" />
          </UiField>
          <UiField label="权重">
            <input v-model.number="item.weight" type="number" min="1" max="100" step="1" />
          </UiField>
          <div class="flex items-end">
            <UiButton variant="ghost" @click="removeJobTemplateDimension(index)">删除</UiButton>
          </div>
        </div>
      </div>
      <UiField label="风险规则（JSON）" help="用于补充岗位级风险判定规则，可为空对象">
        <textarea v-model="jobTemplateRiskRulesText" rows="6" placeholder='{"highRiskKeywords":["频繁跳槽"]}' />
      </UiField>
      <p class="mt-1 mb-2 text-sm" :class="jobTemplateWeightTotal === 100 ? 'text-brand' : 'text-danger'">
        权重合计: {{ jobTemplateWeightTotal }} / 100
      </p>
      <div class="flex items-center gap-2.5 flex-wrap">
        <UiButton variant="secondary" @click="loadJobTemplateOverride">加载模板</UiButton>
        <UiButton @click="saveJobTemplateOverride">保存微调</UiButton>
      </div>
    </UiPanel>

    <UiPanel title="已创建职位">
      <UiTable>
        <thead>
          <tr>
            <UiTh>职位</UiTh>
            <UiTh>公司</UiTh>
            <UiTh>城市</UiTh>
            <UiTh>薪资</UiTh>
            <UiTh>更新时间</UiTh>
          </tr>
        </thead>
        <tbody>
          <tr v-for="job in store.jobs" :key="job.id">
            <UiTd>{{ job.title }}</UiTd>
            <UiTd>{{ job.company }}</UiTd>
            <UiTd>{{ job.city || "-" }}</UiTd>
            <UiTd>{{ job.salary_k || "-" }}</UiTd>
            <UiTd no-wrap>{{ job.updated_at }}</UiTd>
          </tr>
        </tbody>
      </UiTable>
    </UiPanel>
  </section>
</template>
