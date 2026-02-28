<script setup lang="ts">
import { computed } from "vue";
import UiButton from "./UiButton.vue";
import { clampPage, getTotalPages } from "../lib/table-pagination";

const props = withDefaults(
  defineProps<{
    page: number;
    pageSize: number;
    total: number;
    disabled?: boolean;
    pageSizeOptions?: number[];
    showPageSize?: boolean;
  }>(),
  {
    disabled: false,
    pageSizeOptions: () => [10, 20, 50],
    showPageSize: true,
  },
);

const emit = defineEmits<{
  (event: "update:page", value: number): void;
  (event: "update:pageSize", value: number): void;
}>();

const totalPages = computed(() => getTotalPages(props.total, props.pageSize));
const currentPage = computed(() => clampPage(props.page, props.total, props.pageSize));
const pageSizeOptionValues = computed(() => {
  const values = new Set<number>();
  for (const item of props.pageSizeOptions) {
    const value = Math.trunc(Number(item));
    if (Number.isFinite(value) && value > 0) {
      values.add(value);
    }
  }
  values.add(Math.max(1, Math.trunc(props.pageSize || 1)));
  return Array.from(values).sort((a, b) => a - b);
});

function updatePage(nextPage: number) {
  emit("update:page", clampPage(nextPage, props.total, props.pageSize));
}

function onPageSizeChange(event: Event) {
  const target = event.target as HTMLSelectElement;
  const nextPageSize = Math.max(1, Math.trunc(Number(target.value) || props.pageSize));
  emit("update:pageSize", nextPageSize);
  emit("update:page", 1);
}
</script>

<template>
  <div class="mt-3 flex items-center justify-between gap-2 flex-wrap">
    <span class="text-sm text-muted">第 {{ currentPage }} / {{ totalPages }} 页，共 {{ total }} 条</span>
    <div class="flex items-center gap-2 flex-nowrap">
      <label v-if="showPageSize" class="inline-flex items-center gap-1.5 text-sm text-muted whitespace-nowrap shrink-0">
        <span class="whitespace-nowrap">每页</span>
        <select
          class="min-w-14 px-2 py-1 text-sm"
          :value="String(pageSize)"
          :disabled="disabled"
          @change="onPageSizeChange"
        >
          <option v-for="value in pageSizeOptionValues" :key="value" :value="String(value)">
            {{ value }}
          </option>
        </select>
      </label>
      <UiButton
        variant="ghost"
        size="sm"
        :disabled="disabled || currentPage <= 1"
        @click="updatePage(currentPage - 1)"
      >
        上一页
      </UiButton>
      <UiButton
        variant="ghost"
        size="sm"
        :disabled="disabled || currentPage >= totalPages"
        @click="updatePage(currentPage + 1)"
      >
        下一页
      </UiButton>
    </div>
  </div>
</template>
