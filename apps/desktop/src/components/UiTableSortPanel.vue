<script setup lang="ts">
import { computed } from "vue";
import type { SortRule } from "@doss/shared";
import UiButton from "./UiButton.vue";
import UiSelect from "./UiSelect.vue";

interface SortOption {
  label: string;
  value: string;
}

const props = withDefaults(
  defineProps<{
    modelValue: SortRule[];
    options: SortOption[];
    maxRules?: number;
    title?: string;
  }>(),
  {
    maxRules: 3,
    title: "排序设置",
  },
);

const emit = defineEmits<{
  (event: "update:modelValue", value: SortRule[]): void;
  (event: "close"): void;
}>();

const directionOptions = [
  { label: "升序", value: "asc" },
  { label: "降序", value: "desc" },
];

const canAddRule = computed(() =>
  props.modelValue.length < props.maxRules,
);

function updateRules(next: SortRule[]) {
  emit("update:modelValue", next);
}

function addRule() {
  if (!canAddRule.value || props.options.length === 0) {
    return;
  }

  const used = new Set(props.modelValue.map((rule) => rule.field));
  const fallback = props.options.find((item) => !used.has(item.value)) ?? props.options[0]!;
  updateRules([
    ...props.modelValue,
    {
      field: fallback.value,
      direction: "asc",
    },
  ]);
}

function removeRule(index: number) {
  const next = props.modelValue.slice();
  next.splice(index, 1);
  updateRules(next);
}

function moveRule(index: number, direction: "up" | "down") {
  const target = direction === "up" ? index - 1 : index + 1;
  if (target < 0 || target >= props.modelValue.length) {
    return;
  }

  const next = props.modelValue.slice();
  const [rule] = next.splice(index, 1);
  next.splice(target, 0, rule!);
  updateRules(next);
}

function updateField(index: number, field: string | number | boolean) {
  const next = props.modelValue.slice();
  const current = next[index];
  if (!current) {
    return;
  }
  next[index] = {
    ...current,
    field: String(field),
  };
  updateRules(next);
}

function updateDirection(index: number, raw: string | number | boolean) {
  const next = props.modelValue.slice();
  const current = next[index];
  if (!current) {
    return;
  }
  next[index] = {
    ...current,
    direction: String(raw) === "asc" ? "asc" : "desc",
  };
  updateRules(next);
}

function resetRules() {
  updateRules([]);
}

function fieldOptionsFor(index: number): SortOption[] {
  const used = new Set(
    props.modelValue
      .map((rule, currentIndex) => (currentIndex === index ? "" : rule.field))
      .filter(Boolean),
  );

  return props.options.map((item) => ({
    ...item,
    disabled: used.has(item.value),
  })) as SortOption[];
}
</script>

<template>
  <div class="mb-3 rounded-xl border border-line bg-white/90 p-3">
    <div class="mb-2 flex items-center justify-between gap-2">
      <strong class="text-[0.92rem]">{{ title }}</strong>
      <div class="flex items-center gap-2">
        <UiButton variant="ghost" size="sm" @click="resetRules">重置</UiButton>
        <UiButton variant="ghost" size="sm" @click="emit('close')">关闭</UiButton>
      </div>
    </div>

    <div v-if="modelValue.length === 0" class="mb-2 text-[0.82rem] text-muted">
      当前使用默认排序
    </div>

    <div class="flex flex-col gap-2">
      <div
        v-for="(rule, index) in modelValue"
        :key="`${index}-${rule.field}-${rule.direction}`"
        class="grid grid-cols-[1fr_120px_auto] gap-2 lt-md:grid-cols-1"
      >
        <UiSelect
          :model-value="rule.field"
          :options="fieldOptionsFor(index)"
          @update:model-value="updateField(index, $event)"
        />
        <UiSelect
          :model-value="rule.direction"
          :options="directionOptions"
          @update:model-value="updateDirection(index, $event)"
        />
        <div class="flex items-center gap-1 lt-md:justify-start">
          <UiButton variant="ghost" size="sm" :disabled="index === 0" @click="moveRule(index, 'up')">上移</UiButton>
          <UiButton variant="ghost" size="sm" :disabled="index === modelValue.length - 1" @click="moveRule(index, 'down')">下移</UiButton>
          <UiButton variant="ghost" size="sm" @click="removeRule(index)">删除</UiButton>
        </div>
      </div>
    </div>

    <div class="mt-2">
      <UiButton size="sm" :disabled="!canAddRule" @click="addRule">新增排序条件</UiButton>
    </div>
  </div>
</template>
