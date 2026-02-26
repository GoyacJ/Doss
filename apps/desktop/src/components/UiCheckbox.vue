<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    modelValue: boolean;
    label: string;
    disabled?: boolean;
  }>(),
  {
    disabled: false,
  },
);

const emit = defineEmits<{
  (event: "update:modelValue", value: boolean): void;
}>();

function onChange(event: Event) {
  const target = event.target as HTMLInputElement;
  emit("update:modelValue", target.checked);
}
</script>

<template>
  <label
    class="inline-flex items-center gap-2.5 text-[15px] leading-[1.45] text-text select-none"
    :class="props.disabled ? 'cursor-not-allowed opacity-65' : 'cursor-pointer'"
  >
    <input
      :checked="props.modelValue"
      :disabled="props.disabled"
      type="checkbox"
      @change="onChange"
    />
    <span class="leading-[1.35]">{{ props.label }}</span>
  </label>
</template>
