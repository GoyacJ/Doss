<script setup lang="ts">
import { computed, useAttrs } from "vue";

type ButtonVariant = "primary" | "secondary" | "ghost";

const props = withDefaults(
  defineProps<{
    variant?: ButtonVariant;
    disabled?: boolean;
  }>(),
  {
    variant: "primary",
    disabled: false,
  },
);

const attrs = useAttrs();

const baseClass =
  "rounded-xl px-3.5 py-2.5 text-text cursor-pointer transition-colors duration-200 disabled:cursor-not-allowed disabled:opacity-60 border";

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
    :class="[baseClass, variantClass]"
  >
    <slot />
  </button>
</template>
