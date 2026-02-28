<script setup lang="ts">
import UiButton from "./UiButton.vue";

withDefaults(
  defineProps<{
    quickKeyword: string;
    quickPlaceholder?: string;
    disabled?: boolean;
    advancedOpen?: boolean;
    showAdvancedToggle?: boolean;
    showRefresh?: boolean;
    showApply?: boolean;
    showSort?: boolean;
  }>(),
  {
    quickPlaceholder: "输入关键词",
    disabled: false,
    advancedOpen: false,
    showAdvancedToggle: true,
    showRefresh: true,
    showApply: true,
    showSort: false,
  },
);

const emit = defineEmits<{
  (event: "update:quickKeyword", value: string): void;
  (event: "update:advancedOpen", value: boolean): void;
  (event: "apply"): void;
  (event: "refresh"): void;
  (event: "open-sort"): void;
}>();

function onQuickInput(event: Event) {
  const target = event.target as HTMLInputElement;
  emit("update:quickKeyword", target.value);
}
</script>

<template>
  <div class="mb-3 flex flex-col gap-2.5">
    <div class="flex flex-wrap items-end gap-2.5">
      <label class="min-w-[170px] flex-1 max-w-64 flex flex-col gap-1 text-sm text-text">
        <input
          class="table-toolbar-input"
          :value="quickKeyword"
          :placeholder="quickPlaceholder"
          :disabled="disabled"
          @input="onQuickInput"
          @keyup.enter="emit('apply')"
        />
      </label>

      <div class="flex flex-wrap items-center gap-2">
        <UiButton v-if="showApply" variant="secondary" size="sm" :disabled="disabled" @click="emit('apply')">查询</UiButton>
        <UiButton v-if="showRefresh" variant="ghost" size="sm" :disabled="disabled" @click="emit('refresh')">刷新</UiButton>
        <UiButton v-if="showSort" variant="ghost" size="sm" :disabled="disabled" @click="emit('open-sort')">排序</UiButton>
      </div>
    </div>
  </div>
</template>

<style scoped>
.table-toolbar-input {
  font-size: 0.86rem;
  line-height: 1.15;
  padding: 5px 10px;
  min-height: 29px;
  border-color: rgb(23 32 42 / 15%);
  background: rgb(255 255 255 / 78%);
}

.table-toolbar-input:focus {
  border-color: rgb(10 95 84 / 34%);
  box-shadow: 0 0 0 2px rgb(10 95 84 / 10%);
}
</style>
