<script setup lang="ts">
withDefaults(
  defineProps<{
    label: string;
    value: number | string;
  }>(),
  {},
);
</script>

<template>
  <article class="border border-line rounded-[14px] bg-card p-3">
    <p class="m-0 text-muted text-[0.85rem]">{{ label }}</p>
    <h3 class="mt-2 text-[1.4rem] font-700">{{ value }}</h3>
  </article>
</template>
