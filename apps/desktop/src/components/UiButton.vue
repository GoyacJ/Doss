<script setup lang="ts">
import { computed, useAttrs } from "vue";

type ButtonVariant = "primary" | "secondary" | "ghost";
type ButtonSize = "md" | "sm";

const props = withDefaults(
  defineProps<{
    variant?: ButtonVariant;
    size?: ButtonSize;
    disabled?: boolean;
  }>(),
  {
    variant: "primary",
    size: "md",
    disabled: false,
  },
);

const attrs = useAttrs();

const baseClass
  = "text-text cursor-pointer transition-colors duration-200 disabled:cursor-not-allowed disabled:opacity-60 border inline-flex items-center justify-center";

const sizeClass = computed(() => {
  if (props.size === "sm") {
    return "h-7 rounded-lg px-2.5 text-[0.82rem] leading-none";
  }
  return "min-h-10 rounded-xl px-3.5 py-2.5 text-[0.95rem] leading-none";
});

const variantClass = computed(() => {
  if (props.variant === "secondary") {
    return "border-accent/40 bg-accent/14 hover:bg-accent/22";
  }
  if (props.variant === "ghost") {
    return "border-text/20 bg-white/35 hover:bg-white/52";
  }
  return "border-brand/28 bg-brand/10 hover:bg-brand/16";
});
</script>

<template>
  <button
    v-bind="attrs"
    :disabled="props.disabled"
    :class="[baseClass, sizeClass, variantClass]"
  >
    <slot />
  </button>
</template>
