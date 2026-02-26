<script setup lang="ts">
defineProps<{
  label: string;
  help?: string;
}>();
</script>

<template>
  <label class="flex flex-col gap-1.5 text-[15px] leading-[1.45] text-text">
    <span class="font-600">{{ label }}</span>
    <slot />
    <span v-if="help" class="text-[0.82rem] text-muted">{{ help }}</span>
  </label>
</template>
