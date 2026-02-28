<script setup lang="ts">
import { computed } from "vue";
import type { SortDirection, SortRule } from "@doss/shared";

const props = withDefaults(
  defineProps<{
    align?: "left" | "center" | "right";
    sortField?: string;
    sorts?: SortRule[];
  }>(),
  {
    align: "center",
    sortField: undefined,
    sorts: () => [],
  },
);

const emit = defineEmits<{
  (event: "sort", payload: { field: string; direction: SortDirection }): void;
}>();

const activeSortIndex = computed(() => {
  if (!props.sortField) {
    return -1;
  }
  return props.sorts.findIndex((rule) => rule.field === props.sortField);
});

const activeSortDirection = computed<SortDirection | null>(() => {
  const index = activeSortIndex.value;
  if (index < 0) {
    return null;
  }
  return props.sorts[index]?.direction === "asc" ? "asc" : "desc";
});

const alignClass = computed(() => {
  if (props.align === "left") {
    return "justify-start";
  }
  if (props.align === "right") {
    return "justify-end";
  }
  return "justify-center";
});

function triggerSort(direction: SortDirection) {
  if (!props.sortField) {
    return;
  }
  emit("sort", {
    field: props.sortField,
    direction,
  });
}
</script>

<template>
  <th
    :class="[
      'border-b border-line/28 px-2 py-0.5 font-600 text-muted align-middle',
      props.align === 'left' ? 'text-left' : props.align === 'center' ? 'text-center' : 'text-right',
    ]"
  >
    <div
      v-if="props.sortField"
      class="inline-flex w-full items-center gap-1"
      :class="alignClass"
    >
      <span><slot /></span>
      <div class="th-sort-stack">
        <button
          type="button"
          class="th-sort-btn"
          :class="activeSortDirection === 'asc' ? 'is-active' : ''"
          @click.stop="triggerSort('asc')"
        >
          <span class="th-triangle th-triangle-up" />
        </button>
        <button
          type="button"
          class="th-sort-btn"
          :class="activeSortDirection === 'desc' ? 'is-active' : ''"
          @click.stop="triggerSort('desc')"
        >
          <span class="th-triangle th-triangle-down" />
        </button>
      </div>
    </div>
    <slot v-else />
  </th>
</template>

<style scoped>
.th-sort-stack {
  display: inline-flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0;
}

.th-sort-btn {
  all: unset;
  width: 12px;
  height: 8px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: rgb(85 97 112 / 62%);
  cursor: pointer;
}

.th-sort-btn:hover {
  color: rgb(10 95 84 / 80%);
}

.th-sort-btn.is-active {
  color: rgb(10 95 84 / 92%);
}

.th-triangle {
  width: 0;
  height: 0;
  border-left: 3px solid transparent;
  border-right: 3px solid transparent;
}

.th-triangle-up {
  border-bottom: 5px solid currentColor;
}

.th-triangle-down {
  border-top: 5px solid currentColor;
}
</style>
