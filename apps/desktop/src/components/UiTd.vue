<script setup lang="ts">
withDefaults(
  defineProps<{
    align?: "left" | "center" | "right";
    noWrap?: boolean;
  }>(),
  {
    align: "center",
    noWrap: false,
  },
);
</script>

<template>
  <td
    :class="[
      'border-b border-line/18 px-2 py-1.5 align-middle transition-colors duration-180',
      align === 'left' ? 'text-left' : align === 'center' ? 'text-center' : 'text-right',
      noWrap ? 'whitespace-nowrap' : '',
    ]"
  >
    <slot />
  </td>
</template>
