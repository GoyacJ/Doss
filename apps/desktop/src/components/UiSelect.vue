<script setup lang="ts">
import { computed } from "vue";

export interface UiSelectOption {
  label: string;
  value: string | number | boolean;
  disabled?: boolean;
}

const props = withDefaults(defineProps<{
  modelValue: string | number | boolean;
  options: UiSelectOption[];
  valueType?: "string" | "number" | "boolean";
  placeholder?: string;
  disabled?: boolean;
}>(), {
  valueType: "string",
  placeholder: undefined,
  disabled: false,
});

const emit = defineEmits<{
  (event: "update:modelValue", value: string | number | boolean): void;
}>();

const normalizedValue = computed(() => String(props.modelValue ?? ""));

function castValue(rawValue: string): string | number | boolean {
  if (props.valueType === "number") {
    if (!rawValue.trim()) {
      return 0;
    }
    const parsed = Number(rawValue);
    return Number.isFinite(parsed) ? parsed : 0;
  }

  if (props.valueType === "boolean") {
    return rawValue === "true";
  }

  return rawValue;
}

function onChange(event: Event) {
  const target = event.target as HTMLSelectElement;
  emit("update:modelValue", castValue(target.value));
}
</script>

<template>
  <div class="relative">
    <select class="ui-select" :value="normalizedValue" :disabled="disabled" @change="onChange">
      <option v-if="placeholder !== undefined" value="">{{ placeholder }}</option>
      <option
        v-for="option in options"
        :key="String(option.value)"
        :value="String(option.value)"
        :disabled="option.disabled"
      >
        {{ option.label }}
      </option>
    </select>
    <span class="ui-select-chevron" aria-hidden="true" />
  </div>
</template>

<style scoped>
.ui-select {
  appearance: none;
  -webkit-appearance: none;
  -moz-appearance: none;
  padding-right: 2.25rem;
  background-image: none;
}

.ui-select::-ms-expand {
  display: none;
}

.ui-select-chevron {
  position: absolute;
  right: 12px;
  top: 50%;
  width: 7px;
  height: 7px;
  border-right: 1.5px solid rgb(85 97 112 / 78%);
  border-bottom: 1.5px solid rgb(85 97 112 / 78%);
  transform: translateY(-62%) rotate(45deg);
  pointer-events: none;
}

.ui-select:disabled + .ui-select-chevron {
  opacity: 0.5;
}
</style>
