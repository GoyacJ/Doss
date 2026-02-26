<script setup lang="ts">
withDefaults(
  defineProps<{
    label: string;
    value?: string | number | boolean;
    muted?: boolean;
  }>(),
  {
    value: undefined,
    muted: false,
  },
);
</script>

<template>
  <p class="m-0 text-[0.95rem] leading-[1.45]" :class="muted ? 'text-muted' : 'text-text'">
    <span class="font-600">{{ label }}:</span>
    <template v-if="value !== undefined">
      <span class="ml-1">{{ value }}</span>
    </template>
    <template v-else>
      <span class="ml-1">
        <slot />
      </span>
    </template>
  </p>
</template>
