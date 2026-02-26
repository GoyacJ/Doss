<script setup lang="ts">
withDefaults(
  defineProps<{
    align?: "left" | "center" | "right";
  }>(),
  {
    align: "left",
  },
);
</script>

<template>
  <th
    :class="[
      'border-b border-line p-2 font-600 text-muted',
      align === 'left' ? 'text-left' : align === 'center' ? 'text-center' : 'text-right',
    ]"
  >
    <slot />
  </th>
</template>
