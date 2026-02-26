<script setup lang="ts">
withDefaults(
  defineProps<{
    align?: "left" | "center" | "right";
    noWrap?: boolean;
  }>(),
  {
    align: "left",
    noWrap: false,
  },
);
</script>

<template>
  <td
    :class="[
      'border-b border-line p-2 align-top',
      align === 'left' ? 'text-left' : align === 'center' ? 'text-center' : 'text-right',
      noWrap ? 'whitespace-nowrap' : '',
    ]"
  >
    <slot />
  </td>
</template>
