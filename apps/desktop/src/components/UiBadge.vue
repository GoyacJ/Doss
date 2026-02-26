<script setup lang="ts">
type BadgeTone = "neutral" | "info" | "success" | "warning" | "danger";

const props = withDefaults(
  defineProps<{
    tone?: BadgeTone;
    size?: "sm" | "md";
  }>(),
  {
    tone: "neutral",
    size: "sm",
  },
);
</script>

<template>
  <span
    :class="[
      'inline-flex items-center border rounded-full whitespace-nowrap font-600',
      props.size === 'sm' ? 'px-2 py-0.5 text-[0.74rem] leading-[1.1]' : 'px-2.5 py-1 text-[0.82rem] leading-[1.2]',
      props.tone === 'neutral' ? 'text-muted border-line bg-white/45' : '',
      props.tone === 'info' ? 'text-brand border-brand/24 bg-brand/8' : '',
      props.tone === 'success' ? 'text-brand border-brand/30 bg-brand/12' : '',
      props.tone === 'warning' ? 'text-accent border-accent/35 bg-accent/12' : '',
      props.tone === 'danger' ? 'text-danger border-danger/35 bg-danger/12' : '',
    ]"
  >
    <slot />
  </span>
</template>
