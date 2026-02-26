<script setup lang="ts">
defineProps<{
  title?: string;
}>();
</script>

<template>
  <article class="border border-line rounded-2xl bg-card backdrop-blur p-4">
    <slot name="header">
      <h3 v-if="title" class="text-lg font-700 mb-3">{{ title }}</h3>
    </slot>
    <slot />
  </article>
</template>
