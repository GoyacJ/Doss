<script setup lang="ts">
withDefaults(
  defineProps<{
    spacing?: "default" | "comfortable";
  }>(),
  {
    spacing: "default",
  },
);
</script>

<template>
  <div
    :class="[
      'ui-table-wrap w-full overflow-x-auto rounded-xl border border-line/20 bg-white/38',
      spacing === 'comfortable' ? 'px-2 py-2' : 'px-1 py-1',
    ]"
  >
    <table class="ui-table table-auto w-max min-w-full border-collapse text-[0.9rem] leading-5">
      <slot />
    </table>
  </div>
</template>

<style scoped>
.ui-table-wrap {
  box-shadow:
    inset 0 1px 0 rgb(255 255 255 / 55%),
    inset 0 -1px 0 rgb(23 32 42 / 6%);
}

.ui-table-wrap :deep(thead th) {
  background: linear-gradient(
    180deg,
    rgb(255 255 255 / 44%) 0%,
    rgb(255 255 255 / 26%) 100%
  );
}

.ui-table-wrap :deep(tbody tr:nth-child(even) td) {
  background: rgb(255 255 255 / 8%);
}

.ui-table-wrap :deep(tbody tr:hover td) {
  background: rgb(255 255 255 / 24%);
}

.ui-table-wrap :deep(tbody td button) {
  min-height: 27px;
  padding: 0 9px;
  border-radius: 9px;
  font-size: 0.82rem;
}
</style>
