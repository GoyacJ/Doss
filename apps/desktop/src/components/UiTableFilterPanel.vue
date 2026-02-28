<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    open: boolean;
    title?: string;
  }>(),
  {
    title: "高级筛选",
  },
);

const emit = defineEmits<{
  (event: "update:open", value: boolean): void;
}>();

function toggle() {
  emit("update:open", !props.open);
}
</script>

<template>
  <div class="mb-3 rounded-xl border border-line/24 bg-white/30 shadow-[inset_0_1px_0_rgba(255,255,255,0.45)]">
    <button
      type="button"
      class="w-full border-none bg-transparent px-3 py-1.5 text-left text-sm font-600 text-text cursor-pointer"
      @click="toggle"
    >
      {{ title }}
      <span class="ml-2 text-muted">{{ open ? "▲" : "▼" }}</span>
    </button>
    <div v-if="open" class="border-t border-line/20 px-3 py-2.5">
      <slot />
    </div>
  </div>
</template>

<style scoped>
.rounded-xl :deep(input:not([type="checkbox"]):not([type="radio"])),
.rounded-xl :deep(select),
.rounded-xl :deep(textarea) {
  font-size: 0.9rem;
  line-height: 1.25;
  padding: 7px 10px;
}

.rounded-xl :deep(input:not([type="checkbox"]):not([type="radio"])),
.rounded-xl :deep(select) {
  min-height: 33px;
}
</style>
