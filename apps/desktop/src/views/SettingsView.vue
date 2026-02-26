<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import { useRecruitingStore } from "../stores/recruiting";
import {
  deleteAiProviderProfile,
  getAiProviderCatalog,
  listAiProviderProfiles,
  setDefaultAiProviderProfile,
  testAiProviderProfile,
  upsertAiProviderProfile,
  type AiProviderCatalogItem,
  type AiProviderId,
  type AiProviderProfile,
} from "../services/backend";
import { sidecarHealthBadge } from "../lib/status";
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

const loadingCatalog = ref(false);
const loadingProfiles = ref(false);
const savingProfile = ref(false);
const deletingProfileId = ref<string | null>(null);
const testingProfileId = ref<string | null>(null);
const settingDefaultProfileId = ref<string | null>(null);
const providerCatalog = ref<AiProviderCatalogItem[]>([]);
const profiles = ref<AiProviderProfile[]>([]);
const screeningLoading = ref(false);
const screeningSaving = ref(false);

const profileModalOpen = ref(false);
const profileModalMode = ref<"create" | "edit">("create");
const editingProfileId = ref<string | null>(null);
const hydratingForm = ref(false);
const apiKeyInput = ref("");
const clearApiKey = ref(false);
const deleteConfirmProfile = ref<AiProviderProfile | null>(null);

const fallbackCatalog: AiProviderCatalogItem[] = [
  {
    id: "qwen",
    label: "千问 Qwen",
    default_model: "qwen-plus-latest",
    default_base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    models: ["qwen3-max-preview", "qwen-plus-latest", "qwen-turbo-latest", "qwen-flash-latest"],
    docs: [],
  },
  {
    id: "doubao",
    label: "豆包 Doubao",
    default_model: "doubao-seed-1-6-250615",
    default_base_url: "https://ark.cn-beijing.volces.com/api/v3",
    models: [
      "doubao-seed-1-6-250615",
      "doubao-seed-1-6-thinking-250715",
      "doubao-seed-1-6-flash-250715",
    ],
    docs: [],
  },
  {
    id: "deepseek",
    label: "DeepSeek",
    default_model: "deepseek-chat",
    default_base_url: "https://api.deepseek.com",
    models: ["deepseek-chat", "deepseek-reasoner"],
    docs: [],
  },
  {
    id: "minimax",
    label: "MiniMax",
    default_model: "MiniMax-M2.5",
    default_base_url: "https://api.minimaxi.com/v1",
    models: ["MiniMax-M2.5", "MiniMax-M2.5-Flash", "MiniMax-M2.5-highspeed"],
    docs: [],
  },
  {
    id: "glm",
    label: "GLM",
    default_model: "glm-5-air",
    default_base_url: "https://open.bigmodel.cn/api/paas/v4",
    models: ["glm-5", "glm-5-air", "glm-5-flash", "glm-4.5"],
    docs: [],
  },
  {
    id: "openapi",
    label: "OpenApi",
    default_model: "gpt-4.1-mini",
    default_base_url: "https://api.openai.com/v1",
    models: ["gpt-5", "gpt-5-mini", "gpt-4.1", "gpt-4.1-mini", "o4-mini"],
    docs: [],
  },
];

const profileForm = reactive({
  name: "",
  provider: "qwen" as AiProviderId,
  model: "",
  base_url: "",
  temperature: 0.2,
  max_tokens: 1500,
  timeout_secs: 35,
  retry_count: 2,
  has_api_key: false,
});

const selectedProvider = computed(() => providerCatalog.value.find((item) => item.id === profileForm.provider) ?? null);
const selectedProviderModels = computed(() => selectedProvider.value?.models ?? []);
const sidecarBadge = computed(() => sidecarHealthBadge(store.sidecarHealthy));

type ScreeningDimensionFormItem = {
  key: string;
  label: string;
  weight: number;
};

const screeningForm = reactive<{
  name: string;
  dimensions: ScreeningDimensionFormItem[];
}>({
  name: "默认筛选模板",
  dimensions: [
    { key: "goal_orientation", label: "目标导向", weight: 30 },
    { key: "team_collaboration", label: "团队协作", weight: 15 },
    { key: "self_drive", label: "自驱力", weight: 15 },
    { key: "reflection_iteration", label: "反思迭代", weight: 10 },
    { key: "openness", label: "开放性", weight: 8 },
    { key: "resilience", label: "抗压韧性", weight: 7 },
    { key: "learning_ability", label: "学习能力", weight: 10 },
    { key: "values_fit", label: "价值观契合", weight: 5 },
  ],
});
const screeningRiskRulesText = ref("{}");

const screeningWeightTotal = computed(() =>
  screeningForm.dimensions.reduce((sum, item) => sum + Number(item.weight || 0), 0),
);
const canDeleteProfiles = computed(() => profiles.value.length > 1);

function resolveErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  if (typeof error === "string" && error.trim()) {
    return error;
  }
  if (
    typeof error === "object"
    && error !== null
    && "message" in error
    && typeof (error as { message?: unknown }).message === "string"
  ) {
    const message = (error as { message: string }).message.trim();
    if (message) {
      return message;
    }
  }
  return fallback;
}

function applyProviderPreset(providerId: AiProviderId) {
  const preset = providerCatalog.value.find((item) => item.id === providerId);
  if (!preset) {
    return;
  }

  profileForm.model = preset.default_model;
  profileForm.base_url = preset.default_base_url;
  if (profileModalMode.value === "create" && !profileForm.name.trim()) {
    profileForm.name = `${preset.label} 配置`;
  }
}

function openCreateProfileModal() {
  const fallbackProvider = providerCatalog.value[0]?.id ?? "qwen";
  profileModalMode.value = "create";
  editingProfileId.value = null;
  profileModalOpen.value = true;
  hydratingForm.value = true;
  profileForm.provider = fallbackProvider;
  profileForm.name = `${providerCatalog.value.find((item) => item.id === fallbackProvider)?.label ?? "AI"} 配置`;
  profileForm.temperature = 0.2;
  profileForm.max_tokens = 1500;
  profileForm.timeout_secs = 35;
  profileForm.retry_count = 2;
  profileForm.has_api_key = false;
  hydratingForm.value = false;
  applyProviderPreset(fallbackProvider);
  apiKeyInput.value = "";
  clearApiKey.value = false;
}

function openEditProfileModal(profile: AiProviderProfile) {
  profileModalMode.value = "edit";
  editingProfileId.value = profile.id;
  profileModalOpen.value = true;
  hydratingForm.value = true;
  profileForm.name = profile.name;
  profileForm.provider = profile.provider;
  profileForm.model = profile.model;
  profileForm.base_url = profile.base_url;
  profileForm.temperature = profile.temperature;
  profileForm.max_tokens = profile.max_tokens;
  profileForm.timeout_secs = profile.timeout_secs;
  profileForm.retry_count = profile.retry_count;
  profileForm.has_api_key = profile.has_api_key;
  hydratingForm.value = false;
  apiKeyInput.value = "";
  clearApiKey.value = false;
}

function closeProfileModal(force = false) {
  if (savingProfile.value && !force) {
    return;
  }
  profileModalOpen.value = false;
  editingProfileId.value = null;
  apiKeyInput.value = "";
  clearApiKey.value = false;
}

async function loadProviderCatalog() {
  loadingCatalog.value = true;
  try {
    const catalog = await getAiProviderCatalog();
    providerCatalog.value = catalog.providers.length > 0 ? catalog.providers : fallbackCatalog;
  } catch (error) {
    providerCatalog.value = fallbackCatalog;
    const reason = resolveErrorMessage(error, "unknown_error");
    const message = `模型目录加载失败，已使用内置列表: ${reason}`;
    toast.warning(message, 4800);
  } finally {
    loadingCatalog.value = false;
  }
}

async function loadProfiles() {
  loadingProfiles.value = true;
  try {
    profiles.value = await listAiProviderProfiles();
  } catch (error) {
    const message = resolveErrorMessage(error, "加载 AI 配置列表失败");
    toast.danger(message);
  } finally {
    loadingProfiles.value = false;
  }
}

function hydrateScreeningForm() {
  const template = store.activeScreeningTemplate;
  if (!template) {
    return;
  }

  screeningForm.name = template.name || "默认筛选模板";
  if (template.dimensions.length > 0) {
    screeningForm.dimensions = template.dimensions.map((item) => ({
      key: item.key,
      label: item.label,
      weight: item.weight,
    }));
  }
  screeningRiskRulesText.value = JSON.stringify(template.risk_rules ?? {}, null, 2);
}

async function loadScreeningTemplate() {
  screeningLoading.value = true;
  try {
    await store.loadScreeningTemplate();
    hydrateScreeningForm();
  } catch (error) {
    const message = resolveErrorMessage(error, "加载筛选模板失败");
    toast.danger(message);
  } finally {
    screeningLoading.value = false;
  }
}

async function saveScreeningTemplate() {
  if (screeningWeightTotal.value !== 100) {
    toast.warning(`权重总和必须为100，当前为 ${screeningWeightTotal.value}`);
    return;
  }

  const hasInvalidDimension = screeningForm.dimensions.some(
    (item) => !item.key.trim() || !item.label.trim(),
  );
  if (hasInvalidDimension) {
    toast.warning("请填写完整的维度 key 与名称");
    return;
  }

  let riskRules: Record<string, unknown> = {};
  const riskRulesText = screeningRiskRulesText.value.trim();
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

  screeningSaving.value = true;
  try {
    await store.saveScreeningTemplate({
      name: screeningForm.name.trim() || "默认筛选模板",
      dimensions: screeningForm.dimensions.map((item) => ({
        key: item.key.trim(),
        label: item.label.trim(),
        weight: Number(item.weight),
      })),
      risk_rules: riskRules,
    });
    hydrateScreeningForm();
    toast.success("筛选模板已保存");
  } catch (error) {
    const message = resolveErrorMessage(error, "保存筛选模板失败");
    toast.danger(message);
  } finally {
    screeningSaving.value = false;
  }
}

function addScreeningDimension() {
  const next = screeningForm.dimensions.length + 1;
  screeningForm.dimensions.push({
    key: `custom_dimension_${next}`,
    label: `自定义维度${next}`,
    weight: 5,
  });
}

function removeScreeningDimension(index: number) {
  if (screeningForm.dimensions.length <= 1) {
    toast.warning("至少保留一个维度");
    return;
  }
  screeningForm.dimensions.splice(index, 1);
}

async function saveProfile() {
  const profileId = editingProfileId.value;
  if (profileModalMode.value === "edit" && !profileId) {
    toast.danger("未找到要编辑的配置");
    return;
  }

  savingProfile.value = true;
  try {
    const profileIdForPayload = profileModalMode.value === "edit" ? profileId ?? undefined : undefined;
    const payload = {
      profile_id: profileIdForPayload,
      name: profileForm.name.trim() || undefined,
      provider: profileForm.provider,
      model: profileForm.model.trim(),
      base_url: profileForm.base_url.trim(),
      temperature: Number(profileForm.temperature),
      max_tokens: Number(profileForm.max_tokens),
      timeout_secs: Number(profileForm.timeout_secs),
      retry_count: Number(profileForm.retry_count),
      api_key: clearApiKey.value ? "" : apiKeyInput.value.trim() || undefined,
    } as const;

    await upsertAiProviderProfile(payload);
    await loadProfiles();
    await store.loadAiSettings().catch(() => undefined);
    toast.success(profileModalMode.value === "create" ? "AI 配置已新增" : "AI 配置已更新");
    closeProfileModal(true);
  } catch (error) {
    const message = resolveErrorMessage(error, "保存 AI 配置失败");
    toast.danger(message);
  } finally {
    savingProfile.value = false;
  }
}

function askRemoveProfile(profile: AiProviderProfile) {
  if (!canDeleteProfiles.value) {
    toast.warning("至少保留一条 AI 配置");
    return;
  }

  deleteConfirmProfile.value = profile;
}

function cancelRemoveProfile() {
  if (deletingProfileId.value) {
    return;
  }
  deleteConfirmProfile.value = null;
}

async function removeProfile() {
  const profile = deleteConfirmProfile.value;
  if (!profile) {
    return;
  }

  deletingProfileId.value = profile.id;
  try {
    profiles.value = await deleteAiProviderProfile(profile.id);
    await store.loadAiSettings().catch(() => undefined);
    deleteConfirmProfile.value = null;
    toast.success("AI 配置已删除");
  } catch (error) {
    const message = resolveErrorMessage(error, "删除 AI 配置失败");
    toast.danger(message);
  } finally {
    deletingProfileId.value = null;
  }
}

async function runProfileTest(profile: AiProviderProfile) {
  testingProfileId.value = profile.id;
  try {
    const result = await testAiProviderProfile(profile.id);
    const excerpt = result.reply_excerpt.trim() ? `，响应: ${result.reply_excerpt}` : "";
    toast.success(`模型可用（${result.provider}/${result.model}，${result.latency_ms}ms）${excerpt}`, 5200);
  } catch (error) {
    const message = resolveErrorMessage(error, "模型测试失败");
    toast.danger(message);
  } finally {
    testingProfileId.value = null;
  }
}

async function setDefaultProfile(profile: AiProviderProfile) {
  if (profile.is_active) {
    return;
  }

  settingDefaultProfileId.value = profile.id;
  try {
    profiles.value = await setDefaultAiProviderProfile(profile.id);
    await store.loadAiSettings().catch(() => undefined);
    toast.success(`已将「${profile.name}」设为默认模型`);
  } catch (error) {
    const message = resolveErrorMessage(error, "设置默认模型失败");
    toast.danger(message);
  } finally {
    settingDefaultProfileId.value = null;
  }
}

watch(
  () => profileForm.provider,
  (nextProvider, previousProvider) => {
    if (!profileModalOpen.value || hydratingForm.value || nextProvider === previousProvider) {
      return;
    }
    applyProviderPreset(nextProvider);
  },
);

onMounted(async () => {
  try {
    await loadProviderCatalog();
    await loadProfiles();
    await store.loadAiSettings().catch(() => undefined);
    await loadScreeningTemplate();
  } catch (error) {
    const message = resolveErrorMessage(error, "初始化设置失败");
    toast.danger(message);
  }
});
</script>

<template>
  <section class="flex flex-col gap-4">
    <header class="flex items-center justify-between gap-3">
      <h2 class="text-2xl font-700">设置</h2>
    </header>

    <UiPanel>
      <template #header>
        <div class="mb-2 flex items-center justify-between gap-3">
          <h3 class="text-lg font-700">AI配置</h3>
          <UiButton
            variant="secondary"
            :disabled="loadingCatalog || loadingProfiles"
            @click="openCreateProfileModal"
          >
            新增配置
          </UiButton>
        </div>
      </template>

      <p v-if="loadingProfiles" class="m-0 text-muted">配置加载中...</p>

      <template v-else>
        <UiTable v-if="profiles.length > 0">
          <thead>
            <tr>
              <UiTh align="center">名称</UiTh>
              <UiTh align="center">供应商</UiTh>
              <UiTh align="center">模型</UiTh>
              <UiTh align="center">Base URL</UiTh>
              <UiTh align="center">密钥</UiTh>
              <UiTh align="center">操作</UiTh>
            </tr>
          </thead>
          <tbody>
            <tr v-for="profile in profiles" :key="profile.id">
              <UiTd align="center">{{ profile.name }}</UiTd>
              <UiTd align="center">{{ profile.provider }}</UiTd>
              <UiTd align="center">{{ profile.model }}</UiTd>
              <UiTd align="center" class="break-all">{{ profile.base_url }}</UiTd>
              <UiTd align="center">{{ profile.has_api_key ? "已配置" : "未配置" }}</UiTd>
              <UiTd align="center" no-wrap>
                <div class="flex justify-center gap-2">
                  <UiButton
                    variant="ghost"
                    :disabled="profile.is_active || settingDefaultProfileId === profile.id"
                    @click="setDefaultProfile(profile)"
                  >
                    {{ settingDefaultProfileId === profile.id ? "设置中..." : (profile.is_active ? "默认中" : "设为默认") }}
                  </UiButton>
                  <UiButton
                    variant="ghost"
                    :disabled="testingProfileId === profile.id"
                    @click="runProfileTest(profile)"
                  >
                    {{ testingProfileId === profile.id ? "测试中..." : "测试" }}
                  </UiButton>
                  <UiButton variant="ghost" @click="openEditProfileModal(profile)">编辑</UiButton>
                  <UiButton
                    variant="ghost"
                    :disabled="!canDeleteProfiles || deletingProfileId === profile.id"
                    @click="askRemoveProfile(profile)"
                  >
                    {{ deletingProfileId === profile.id ? "删除中..." : "删除" }}
                  </UiButton>
                </div>
              </UiTd>
            </tr>
          </tbody>
        </UiTable>

        <p v-else class="m-0 text-muted">暂无 AI 配置，点击“新增配置”创建第一条。</p>
      </template>
    </UiPanel>

    <UiPanel title="全局筛选模板">
      <UiField label="模板名称">
        <input v-model="screeningForm.name" placeholder="默认筛选模板" />
      </UiField>
      <div class="mt-3 mb-2 flex items-center gap-2">
        <UiButton variant="secondary" :disabled="screeningLoading || screeningSaving" @click="addScreeningDimension">
          新增维度
        </UiButton>
      </div>
      <div class="grid gap-2.5">
        <div
          v-for="(item, index) in screeningForm.dimensions"
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
            <UiButton
              variant="ghost"
              :disabled="screeningLoading || screeningSaving"
              @click="removeScreeningDimension(index)"
            >
              删除
            </UiButton>
          </div>
        </div>
      </div>
      <UiField label="风险规则（JSON）" help="用于补充全局风险规则，可为空对象" class="mt-3">
        <textarea v-model="screeningRiskRulesText" rows="6" placeholder='{"highRiskKeywords":["频繁跳槽"]}' />
      </UiField>
      <p class="mt-3 mb-2 text-sm" :class="screeningWeightTotal === 100 ? 'text-brand' : 'text-danger'">
        权重合计: {{ screeningWeightTotal }} / 100
      </p>
      <div class="flex items-center gap-2.5 flex-wrap">
        <UiButton
          variant="secondary"
          :disabled="screeningLoading || screeningSaving"
          @click="loadScreeningTemplate"
        >
          {{ screeningLoading ? "加载中..." : "重新加载模板" }}
        </UiButton>
        <UiButton :disabled="screeningLoading || screeningSaving" @click="saveScreeningTemplate">
          {{ screeningSaving ? "保存中..." : "保存模板" }}
        </UiButton>
      </div>
    </UiPanel>

    <UiPanel title="系统状态">
      <div class="flex flex-col gap-1.5">
        <UiInfoRow label="数据库路径" :value="store.health?.dbPath || '-'" />
        <UiInfoRow label="Sidecar">
          <UiBadge :tone="sidecarBadge.tone">{{ sidecarBadge.label }}</UiBadge>
        </UiInfoRow>
      </div>
    </UiPanel>

    <div
      v-if="profileModalOpen"
      class="fixed inset-0 z-[80] flex items-center justify-center bg-black/35 p-4"
      @click.self="closeProfileModal()"
    >
      <div class="w-full max-w-3xl">
        <UiPanel :title="profileModalMode === 'create' ? '新增 AI 配置' : '编辑 AI 配置'">
          <div class="grid grid-cols-2 gap-3 lt-lg:grid-cols-1">
            <UiField label="配置名称">
              <input v-model="profileForm.name" placeholder="例如：主流程-DeepSeek" />
            </UiField>
            <UiField label="供应商">
              <div class="relative">
                <select
                  v-model="profileForm.provider"
                  class="appearance-none pr-9 bg-white/95"
                  :disabled="loadingCatalog || savingProfile"
                >
                  <option v-for="provider in providerCatalog" :key="provider.id" :value="provider.id">
                    {{ provider.label }}
                  </option>
                </select>
                <span class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-muted text-sm">▾</span>
              </div>
            </UiField>
            <UiField label="模型">
              <input
                v-model="profileForm.model"
                list="ai-profile-models"
                :placeholder="selectedProvider?.default_model || '输入模型名称'"
              />
            </UiField>
            <UiField label="Base URL">
              <input v-model="profileForm.base_url" :placeholder="selectedProvider?.default_base_url || 'https://...'" />
            </UiField>
            <UiField label="API Key（仅在输入时更新）" help="不输入则保持当前已保存密钥">
              <input v-model="apiKeyInput" type="password" placeholder="sk-..." />
            </UiField>
            <div class="flex items-end">
              <UiCheckbox v-model="clearApiKey" label="清空已保存 API Key" />
            </div>
            <UiField label="Temperature">
              <input v-model.number="profileForm.temperature" type="number" min="0" max="1.2" step="0.1" />
            </UiField>
            <UiField label="Max Tokens">
              <input v-model.number="profileForm.max_tokens" type="number" min="200" max="8192" step="100" />
            </UiField>
            <UiField label="Timeout (sec)">
              <input v-model.number="profileForm.timeout_secs" type="number" min="8" max="180" step="1" />
            </UiField>
            <UiField label="Retry Count">
              <input v-model.number="profileForm.retry_count" type="number" min="1" max="5" step="1" />
            </UiField>
          </div>
          <datalist id="ai-profile-models">
            <option v-for="model in selectedProviderModels" :key="model" :value="model" />
          </datalist>

          <div class="mt-4 flex flex-wrap justify-end gap-2">
            <UiButton variant="ghost" :disabled="savingProfile" @click="closeProfileModal()">取消</UiButton>
            <UiButton
              variant="secondary"
              :disabled="savingProfile || loadingCatalog"
              @click="applyProviderPreset(profileForm.provider)"
            >
              使用默认参数
            </UiButton>
            <UiButton :disabled="savingProfile" @click="saveProfile">
              {{ savingProfile ? "保存中..." : "保存" }}
            </UiButton>
          </div>
        </UiPanel>
      </div>
    </div>

    <div
      v-if="deleteConfirmProfile"
      class="fixed inset-0 z-[85] flex items-center justify-center bg-black/35 p-4"
      @click.self="cancelRemoveProfile()"
    >
      <div class="w-full max-w-md">
        <UiPanel title="删除 AI 配置">
          <p class="m-0">
            确认删除配置「{{ deleteConfirmProfile.name }}」吗？此操作不可撤销。
          </p>

          <div class="mt-4 flex flex-wrap justify-end gap-2">
            <UiButton
              variant="ghost"
              :disabled="deletingProfileId === deleteConfirmProfile.id"
              @click="cancelRemoveProfile()"
            >
              取消
            </UiButton>
            <UiButton
              :disabled="deletingProfileId === deleteConfirmProfile.id"
              @click="removeProfile()"
            >
              {{ deletingProfileId === deleteConfirmProfile.id ? "删除中..." : "确认删除" }}
            </UiButton>
          </div>
        </UiPanel>
      </div>
    </div>
  </section>
</template>
